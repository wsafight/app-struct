struct Allowance {
    entitlement: &'static str,
    free_limit: u32,
}

const ALLOWANCES: &[Allowance] = &[__ALLOWANCES__];

#[derive(Serialize)]
struct AllowanceView {
    entitlement: &'static str,
    free_limit: u32,
    used: u64,
    remaining: u64,
    paid: bool,
}

fn subscription_active(subscription: &SubscriptionView) -> bool {
    matches!(subscription.status.as_str(), "active" | "trialing")
        && subscription
            .current_period_end
            .is_none_or(|end| end > chrono::Utc::now())
}

async fn has_entitlement_on<C: DatabaseExecutor>(
    database: &C,
    organization_id: uuid::Uuid,
    entitlement: &str,
) -> Result<bool, ApiError> {
    let subscription = current_subscription(database, organization_id).await?;
    Ok(subscription.as_ref().is_some_and(|subscription| {
        subscription_active(subscription)
            && PLANS.iter().any(|plan| {
                plan.id == subscription.plan_id && plan.entitlements.contains(&entitlement)
            })
    }))
}

async fn allowance_views(context: &RequestContext<'_>) -> Result<Vec<AllowanceView>, ApiError> {
    let organization_id = context.require_tenant()?;
    let mut result = Vec::new();
    for allowance in ALLOWANCES {
        let used = usage_count(context, organization_id, allowance.entitlement).await?;
        result.push(AllowanceView {
            entitlement: allowance.entitlement,
            free_limit: allowance.free_limit,
            used,
            remaining: u64::from(allowance.free_limit).saturating_sub(used),
            paid: has_entitlement_on(context, organization_id, allowance.entitlement).await?,
        });
    }
    Ok(result)
}

async fn usage_count<C: DatabaseExecutor>(
    database: &C,
    organization_id: uuid::Uuid,
    entitlement: &str,
) -> Result<u64, ApiError> {
    let row = database.fetch_optional(Sql::bind(
        "SELECT COUNT(*) AS total FROM \"_appstruct_billing_usage\" WHERE organization_id = $1 AND entitlement = $2 AND free AND NOT refunded",
        [organization_id.into(), entitlement.to_owned().into()],
    )).await?.ok_or(ApiError::Internal)?;
    Ok(u64::try_from(row.try_get::<i64, _>("total")?).unwrap_or(0))
}

pub(crate) async fn reserve_allowance(
    transaction: &crate::database::Transaction,
    context: &RequestContext<'_>,
    entitlement: &str,
    operation_id: uuid::Uuid,
) -> Result<(), ApiError> {
    context.actor().ok_or(ApiError::Unauthorized)?;
    let organization_id = context.require_tenant()?;
    transaction
        .fetch_optional(Sql::bind(
            "SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE",
            [organization_id.into()],
        ))
        .await?
        .ok_or(ApiError::InvalidTenant)?;
    if let Some(row) = transaction.fetch_optional(Sql::bind(
        "SELECT organization_id, entitlement, refunded FROM \"_appstruct_billing_usage\" WHERE id = $1",
        [operation_id.into()],
    )).await? {
        return if row.try_get::<uuid::Uuid, _>("organization_id")? == organization_id
            && row.try_get::<String, _>("entitlement")? == entitlement
            && !row.try_get::<bool, _>("refunded")?
        { Ok(()) } else { Err(ApiError::Conflict("The usage reservation cannot be reused".to_owned())) };
    }
    let paid = has_entitlement_on(transaction, organization_id, entitlement).await?;
    if !paid {
        let allowance = ALLOWANCES
            .iter()
            .find(|item| item.entitlement == entitlement)
            .ok_or_else(|| ApiError::EntitlementRequired(entitlement.to_owned()))?;
        if usage_count(transaction, organization_id, entitlement).await?
            >= u64::from(allowance.free_limit)
        {
            return Err(ApiError::QuotaExceeded(entitlement.to_owned()));
        }
    }
    transaction.execute(Sql::bind(
        "INSERT INTO \"_appstruct_billing_usage\" (id, organization_id, entitlement, free, refunded, created_at) VALUES ($1, $2, $3, $4, false, CURRENT_TIMESTAMP)",
        [operation_id.into(), organization_id.into(), entitlement.to_owned().into(), (!paid).into()],
    )).await?;
    Ok(())
}

pub(crate) async fn refund_allowance<C: DatabaseExecutor>(
    database: &C,
    operation_id: uuid::Uuid,
) -> Result<(), ApiError> {
    database.execute(Sql::bind(
        "UPDATE \"_appstruct_billing_usage\" u SET refunded = true WHERE u.id = $1 AND NOT u.refunded AND EXISTS (SELECT 1 FROM \"_appstruct_report_runs\" r WHERE r.id = u.id AND r.stage IN ('failed', 'cancelled'))",
        [operation_id.into()],
    )).await?;
    Ok(())
}
