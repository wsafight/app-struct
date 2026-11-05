use super::*;

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/product/notifications", get(list))
        .route("/api/product/notifications/read", post(read_all))
        .route("/api/product/notifications/{id}/read", post(read))
}

pub async fn report_finished<C: DatabaseExecutor>(database: &C, id: uuid::Uuid) -> Result<(), ApiError> {
    if !__REPORT__ { return Ok(()); }
    let row = database.fetch_optional(Sql::bind("SELECT actor_id, tenant_id, stage::text AS stage FROM \"_appstruct_report_runs\" WHERE id = $1 AND stage IN ('succeeded', 'failed', 'cancelled')", [id.into()])).await?;
    if let Some(row) = row {
        if let Some(user) = row.try_get::<Option<uuid::Uuid>, _>("actor_id")? {
            let tenant = row.try_get::<Option<uuid::Uuid>, _>("tenant_id")?;
            let stage = row.try_get::<String, _>("stage")?;
            notify(database, user, tenant, &format!("report:{id}:{stage}"), &format!("Report {stage}"), "Open the results center to download or retry your report.", "/results").await?;
            if stage == "succeeded" { record_event(database, Some(user), None, tenant, &format!("report:{id}"), "activation").await?; }
        }
    }
    Ok(())
}

pub async fn notify<C: DatabaseExecutor>(database: &C, user: uuid::Uuid, tenant: Option<uuid::Uuid>, key: &str, title: &str, body: &str, target: &str) -> Result<(), ApiError> {
    if !target.starts_with('/') || target.starts_with("//") || target.contains('\\') || key.len() > 400 || title.len() > 200 || body.len() > 2000 { return Err(invalid("Invalid notification")); }
    let key = format!("{user}:{key}");
    database.execute(Sql::bind("INSERT INTO \"_appstruct_product_notifications\" (id, user_id, tenant_id, event_key, title, body, target, created_at) SELECT $1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP WHERE COALESCE((SELECT notifications FROM \"_appstruct_product_profiles\" WHERE user_id = $2), true) ON CONFLICT (event_key) DO NOTHING", [uuid::Uuid::now_v7().into(), user.into(), tenant.into(), key.into(), title.into(), body.into(), target.into()])).await?;
    Ok(())
}

async fn list(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<Pagination>) -> Result<Json<Value>, ApiError> {
    let context = state.context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let (limit, offset) = page(&query)?;
    let rows = state.database.fetch_all(Sql::bind("SELECT id, tenant_id, title, body, target, read_at, created_at FROM \"_appstruct_product_notifications\" WHERE user_id = $1 AND (tenant_id IS NULL OR tenant_id = $2) ORDER BY created_at DESC, id DESC LIMIT $3 OFFSET $4", [actor.id.into(), context.tenant().into(), limit.into(), offset.into()])).await?;
    let counts = state.database.fetch_optional(Sql::bind("SELECT COUNT(*) AS total, COUNT(*) FILTER (WHERE read_at IS NULL) AS unread FROM \"_appstruct_product_notifications\" WHERE user_id = $1 AND (tenant_id IS NULL OR tenant_id = $2)", [actor.id.into(), context.tenant().into()])).await?.ok_or(ApiError::Internal)?;
    let data = rows.into_iter().map(|row| Ok(json!({ "id": row.try_get::<uuid::Uuid, _>("id")?, "tenant_id": row.try_get::<Option<uuid::Uuid>, _>("tenant_id")?, "title": row.try_get::<String, _>("title")?, "body": row.try_get::<String, _>("body")?, "target": row.try_get::<String, _>("target")?, "read_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("read_at")?, "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")? }))).collect::<Result<Vec<Value>, ApiError>>()?;
    Ok(Json(json!({ "data": data, "total": counts.try_get::<i64, _>("total")?, "unread": counts.try_get::<i64, _>("unread")? })))
}

async fn read(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<uuid::Uuid>) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let result = state.database.execute(Sql::bind("UPDATE \"_appstruct_product_notifications\" SET read_at = COALESCE(read_at, CURRENT_TIMESTAMP) WHERE id = $1 AND user_id = $2 AND (tenant_id IS NULL OR tenant_id = $3)", [id.into(), actor.id.into(), context.tenant().into()])).await?;
    if result.rows_affected() == 0 { return Err(ApiError::NotFound); }
    Ok(StatusCode::NO_CONTENT)
}

async fn read_all(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    state.database.execute(Sql::bind("UPDATE \"_appstruct_product_notifications\" SET read_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND (tenant_id IS NULL OR tenant_id = $2) AND read_at IS NULL", [actor.id.into(), context.tenant().into()])).await?;
    Ok(StatusCode::NO_CONTENT)
}
