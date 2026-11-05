use super::*;

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/product/team", get(details).patch(rename))
        .route("/api/product/team/transfer", post(transfer))
        .route("/api/product/team/members/{id}", axum::routing::delete(remove))
}

async fn seat_limit<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid) -> Result<i64, ApiError> {
    let plans: &[&str] = &[__SEAT_PLANS__];
    let paid = if __BILLING__ && !plans.is_empty() {
        database.fetch_optional(Sql::bind("SELECT plan_id, status, current_period_end FROM \"_appstruct_billing_subscriptions\" WHERE organization_id = $1 ORDER BY updated_at DESC, id DESC LIMIT 1", [tenant.into()])).await?.is_some_and(|row| row.try_get::<String, _>("plan_id").is_ok_and(|plan| plans.contains(&plan.as_str())) && row.try_get::<String, _>("status").is_ok_and(|status| matches!(status.as_str(), "active" | "trialing")) && row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("current_period_end").is_ok_and(|end| end.is_none_or(|end| end > chrono::Utc::now())))
    } else { false };
    if paid { setting("APPSTRUCT_TEAM_PAID_SEAT_LIMIT", 0, 100000) } else { setting("APPSTRUCT_TEAM_SEAT_LIMIT", 0, 100000) }
}

pub async fn check_capacity<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid) -> Result<(), ApiError> {
    if !__TENANT__ { return Ok(()); }
    database.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
    let limit = seat_limit(database, tenant).await?;
    if limit == 0 { return Ok(()); }
    let count = database.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1", [tenant.into()])).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    if count >= limit { return Err(ApiError::QuotaExceeded("seats".to_owned())); }
    Ok(())
}

async fn details(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let context = state.context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let organization = state.database.fetch_optional(Sql::bind("SELECT name FROM \"_appstruct_tenant_organizations\" WHERE id = $1", [tenant.into()])).await?.ok_or(ApiError::NotFound)?;
    let rows = state.database.fetch_all(Sql::bind("SELECT m.user_id, m.role, m.created_at, p.display_name FROM \"_appstruct_tenant_memberships\" m LEFT JOIN \"_appstruct_product_profiles\" p ON p.user_id = m.user_id WHERE m.organization_id = $1 ORDER BY m.created_at, m.user_id LIMIT 1000", [tenant.into()])).await?;
    let members = rows.iter().map(|row| Ok(json!({ "id": row.try_get::<uuid::Uuid, _>("user_id")?, "name": row.try_get::<Option<String>, _>("display_name")?, "role": row.try_get::<String, _>("role")?, "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")? }))).collect::<Result<Vec<Value>, ApiError>>()?;
    let is_owner = rows.iter().any(|row| row.try_get::<uuid::Uuid, _>("user_id").ok() == Some(actor.id) && row.try_get::<String, _>("role").is_ok_and(|role| role == "owner"));
    let count = state.database.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1", [tenant.into()])).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    Ok(Json(json!({ "id": tenant, "name": organization.try_get::<String, _>("name")?, "is_owner": is_owner, "seat_limit": seat_limit(&state.database, tenant).await?, "seats_used": count, "members": members, "members_truncated": count > 1000 })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameInput { name: String }

async fn rename(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<NameInput>) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 120 { return Err(invalid("Team name must contain 1–120 bytes")); }
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
    owner(&transaction, tenant, actor.id).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_tenant_organizations\" SET name = $2 WHERE id = $1", [tenant.into(), name.into()])).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferInput { member_id: uuid::Uuid }

async fn transfer(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<TransferInput>) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    if input.member_id == actor.id { return Err(invalid("Choose another team member")); }
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
    owner(&transaction, tenant, actor.id).await?;
    let updated = transaction.execute(Sql::bind("UPDATE \"_appstruct_tenant_memberships\" SET role = 'owner' WHERE organization_id = $1 AND user_id = $2", [tenant.into(), input.member_id.into()])).await?;
    if updated.rows_affected() == 0 { return Err(ApiError::NotFound); }
    transaction.execute(Sql::bind("UPDATE \"_appstruct_tenant_memberships\" SET role = 'member' WHERE organization_id = $1 AND user_id = $2", [tenant.into(), actor.id.into()])).await?;
    notify(&transaction, input.member_id, Some(tenant), &format!("owner:{}", uuid::Uuid::now_v7()), "You now own this team", "Review your team and billing settings.", "/team").await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(State(state): State<AppState>, headers: HeaderMap, Path(member): Path<uuid::Uuid>) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
    if actor.id != member { owner(&transaction, tenant, actor.id).await?; }
    let row = transaction.fetch_optional(Sql::bind("SELECT role FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2", [tenant.into(), member.into()])).await?.ok_or(ApiError::NotFound)?;
    if row.try_get::<String, _>("role")? == "owner" { return Err(ApiError::Conflict("Transfer ownership before removing an owner".to_owned())); }
    transaction.execute(Sql::bind("DELETE FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2", [tenant.into(), member.into()])).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
