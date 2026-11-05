use super::*;
use crate::database::Transaction;

pub(super) fn router() -> Router<AppState> { Router::new().route("/api/product/credits", get(overview)) }

async fn lock<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid) -> Result<(), ApiError> {
    if !__TENANT__ { return Err(ApiError::NotFound); }
    database.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
    Ok(())
}

async fn monthly<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid) -> Result<(i64, chrono::DateTime<chrono::Utc>), ApiError> {
    let plans: &[&str] = &[__CREDIT_PLANS__];
    let paid = if __BILLING__ && !plans.is_empty() {
        let row = database.fetch_optional(Sql::bind("SELECT plan_id, status, current_period_end FROM \"_appstruct_billing_subscriptions\" WHERE organization_id = $1 ORDER BY updated_at DESC, id DESC LIMIT 1", [tenant.into()])).await?;
        row.is_some_and(|row| row.try_get::<String, _>("plan_id").is_ok_and(|plan| plans.contains(&plan.as_str())) && row.try_get::<String, _>("status").is_ok_and(|status| matches!(status.as_str(), "active" | "trialing")) && row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("current_period_end").is_ok_and(|end| end.is_none_or(|end| end > chrono::Utc::now())))
    } else { false };
    let allowance = if paid { setting("APPSTRUCT_MONTHLY_PAID_CREDITS", 200, 1_000_000)? } else { setting("APPSTRUCT_MONTHLY_FREE_CREDITS", 20, 1_000_000)? };
    let window = database.fetch_optional(Sql::new("SELECT date_trunc('month', CURRENT_TIMESTAMP AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' AS starts, (date_trunc('month', CURRENT_TIMESTAMP AT TIME ZONE 'UTC') + INTERVAL '1 month') AT TIME ZONE 'UTC' AS ends")).await?.ok_or(ApiError::Internal)?;
    let start = window.try_get::<chrono::DateTime<chrono::Utc>, _>("starts")?;
    let end = window.try_get::<chrono::DateTime<chrono::Utc>, _>("ends")?;
    ledger(database, tenant, None, &format!("month:{tenant}:{}", start.timestamp()), allowance, "monthly", Some(end)).await?;
    database.execute(Sql::bind("UPDATE \"_appstruct_product_credit_ledger\" SET amount = GREATEST(amount, $2) WHERE operation_key = $1", [format!("month:{tenant}:{}", start.timestamp()).into(), allowance.into()])).await?;
    Ok((allowance, end))
}

pub(super) async fn ledger<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid, actor: Option<uuid::Uuid>, key: &str, amount: i64, reason: &str, expires: Option<chrono::DateTime<chrono::Utc>>) -> Result<(), ApiError> {
    database.execute(Sql::bind("INSERT INTO \"_appstruct_product_credit_ledger\" (id, tenant_id, actor_id, operation_key, amount, reason, expires_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP) ON CONFLICT (operation_key) DO NOTHING", [uuid::Uuid::now_v7().into(), tenant.into(), actor.into(), key.into(), amount.into(), reason.into(), expires.into()])).await?;
    Ok(())
}

async fn balances<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid) -> Result<(i64, i64), ApiError> {
    let row = database.fetch_optional(Sql::bind("SELECT COALESCE(SUM(amount) FILTER (WHERE expires_at > CURRENT_TIMESTAMP), 0)::bigint AS monthly, COALESCE(SUM(amount) FILTER (WHERE expires_at IS NULL), 0)::bigint AS paid FROM \"_appstruct_product_credit_ledger\" WHERE tenant_id = $1", [tenant.into()])).await?.ok_or(ApiError::Internal)?;
    Ok((row.try_get("monthly")?, row.try_get("paid")?))
}

pub async fn reserve_credits(transaction: &Transaction, tenant: uuid::Uuid, actor: uuid::Uuid, operation: uuid::Uuid, cost: i64) -> Result<bool, ApiError> {
    if !(1..=1_000_000).contains(&cost) { return Err(invalid("Invalid credit cost")); }
    lock(transaction, tenant).await?;
    if let Some(row) = transaction.fetch_optional(Sql::bind("SELECT tenant_id, actor_id, cost, refunded FROM \"_appstruct_product_credit_reservations\" WHERE id = $1", [operation.into()])).await? {
        if row.try_get::<uuid::Uuid, _>("tenant_id")? != tenant || row.try_get::<uuid::Uuid, _>("actor_id")? != actor || row.try_get::<i64, _>("cost")? != cost || row.try_get::<bool, _>("refunded")? { return Err(ApiError::Conflict("Credit reservation cannot be reused".to_owned())); }
        return Ok(false);
    }
    let (_, end) = monthly(transaction, tenant).await?;
    let (available_monthly, available_paid) = balances(transaction, tenant).await?;
    if available_paid < 0 || available_monthly + available_paid < cost { return Err(ApiError::QuotaExceeded("credits".to_owned())); }
    let from_monthly = cost.min(available_monthly.max(0));
    let from_paid = cost - from_monthly;
    transaction.execute(Sql::bind("INSERT INTO \"_appstruct_product_credit_reservations\" (id, tenant_id, actor_id, cost, monthly, paid, expires_at, refunded, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, false, CURRENT_TIMESTAMP)", [operation.into(), tenant.into(), actor.into(), cost.into(), from_monthly.into(), from_paid.into(), end.into()])).await?;
    ledger(transaction, tenant, Some(actor), &format!("spend:{operation}:monthly"), -from_monthly, "usage", Some(end)).await?;
    ledger(transaction, tenant, Some(actor), &format!("spend:{operation}:paid"), -from_paid, "usage", None).await?;
    Ok(true)
}

pub async fn refund_credits(transaction: &Transaction, tenant: uuid::Uuid, operation: uuid::Uuid) -> Result<(), ApiError> {
    lock(transaction, tenant).await?;
    let row = transaction.fetch_optional(Sql::bind("UPDATE \"_appstruct_product_credit_reservations\" SET refunded = true WHERE id = $1 AND tenant_id = $2 AND NOT refunded RETURNING actor_id, monthly, paid, expires_at", [operation.into(), tenant.into()])).await?;
    if let Some(row) = row {
        let actor = row.try_get::<uuid::Uuid, _>("actor_id")?;
        ledger(transaction, tenant, Some(actor), &format!("refund:{operation}:monthly"), row.try_get("monthly")?, "failure_refund", Some(row.try_get("expires_at")?)).await?;
        ledger(transaction, tenant, Some(actor), &format!("refund:{operation}:paid"), row.try_get("paid")?, "failure_refund", None).await?;
    }
    Ok(())
}

async fn overview(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<Pagination>) -> Result<Json<Value>, ApiError> {
    let context = state.context(&headers).await?;
    context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let (limit, offset) = page(&query)?;
    let transaction = state.database.begin().await?;
    lock(&transaction, tenant).await?;
    let (allowance, end) = monthly(&transaction, tenant).await?;
    let (monthly, paid) = balances(&transaction, tenant).await?;
    let rows = transaction.fetch_all(Sql::bind("SELECT id, amount, reason, expires_at, created_at FROM \"_appstruct_product_credit_ledger\" WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3", [tenant.into(), limit.into(), offset.into()])).await?;
    let data = rows.iter().map(|row| Ok(json!({ "id": row.try_get::<uuid::Uuid, _>("id")?, "amount": row.try_get::<i64, _>("amount")?, "reason": row.try_get::<String, _>("reason")?, "expires_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")?, "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")? }))).collect::<Result<Vec<Value>, ApiError>>()?;
    transaction.commit().await?;
    Ok(Json(json!({ "monthly_allowance": allowance, "monthly_remaining": monthly, "paid_balance": paid, "available": (monthly + paid).max(0), "resets_at": end, "data": data, "page_size": limit })))
}
