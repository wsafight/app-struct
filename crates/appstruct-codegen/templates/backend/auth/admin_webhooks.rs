use super::config;
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/webhooks", get(list_deliveries))
        .route("/api/admin/webhooks/{id}/retry", post(retry_delivery))
        .route("/api/admin/webhooks/{id}/replay", post(replay_delivery))
}

#[derive(Deserialize)]
struct DeliveryQuery {
    status: Option<String>,
    limit: Option<u64>,
}

#[derive(Serialize)]
struct AdminWebhookDelivery {
    id: uuid::Uuid,
    endpoint: String,
    event: String,
    status: String,
    tenant_id: Option<uuid::Uuid>,
    attempts: i32,
    max_attempts: i32,
    next_attempt_at: chrono::DateTime<chrono::Utc>,
    response_status: Option<i32>,
    last_error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct DeliveryList { data: Vec<AdminWebhookDelivery> }

async fn list_deliveries(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<DeliveryQuery>,
) -> Result<Json<DeliveryList>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_enabled()?;
    if input.status.as_deref().is_some_and(|status| {
        !matches!(status, "pending" | "delivering" | "succeeded" | "dead")
    }) {
        return Err(ApiError::InvalidQuery(
            "status must be pending, delivering, succeeded, or dead".to_owned(),
        ));
    }
    let limit = input.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::InvalidQuery("limit must be between 1 and 100".to_owned()));
    }
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, endpoint, event, status::text AS status, tenant_id, attempts, max_attempts, next_attempt_at, response_status, last_error, created_at, completed_at FROM \"_appstruct_webhook_deliveries\" WHERE ($1::text IS NULL OR status::text = $1) ORDER BY created_at DESC, id DESC LIMIT $2",
        [input.status.into(), i64::try_from(limit).unwrap_or(100).into()],
    )).await?;
    let data = rows.into_iter().map(delivery_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(DeliveryList { data }))
}

async fn retry_delivery(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminWebhookDelivery>, ApiError> {
    require_admin_mutation(&state, &headers).await?;
    ensure_enabled()?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    if lock_status(&transaction, id).await? != "dead" {
        return Err(ApiError::Conflict("Only dead webhook deliveries can be retried".to_owned()));
    }
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_webhook_deliveries\" SET status = 'pending', attempts = 0, next_attempt_at = CURRENT_TIMESTAMP, locked_by = NULL, locked_until = NULL, response_status = NULL, last_error = NULL, completed_at = NULL WHERE id = $1 RETURNING id, endpoint, event, status::text AS status, tenant_id, attempts, max_attempts, next_attempt_at, response_status, last_error, created_at, completed_at",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let delivery = delivery_from_row(row)?;
    transaction.commit().await?;
    Ok(Json(delivery))
}

async fn replay_delivery(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<(StatusCode, Json<AdminWebhookDelivery>), ApiError> {
    require_admin_mutation(&state, &headers).await?;
    ensure_enabled()?;
    let source_id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    let status = lock_status(&transaction, source_id).await?;
    if !matches!(status.as_str(), "succeeded" | "dead") {
        return Err(ApiError::Conflict(
            "Only succeeded or dead webhook deliveries can be replayed".to_owned(),
        ));
    }
    let id = uuid::Uuid::now_v7();
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_webhook_deliveries\" (id, endpoint, event, payload, idempotency_key, tenant_id, status, attempts, max_attempts, backoff_seconds, next_attempt_at, created_at) SELECT $1, endpoint, event, payload, NULL, tenant_id, 'pending', 0, max_attempts, backoff_seconds, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP FROM \"_appstruct_webhook_deliveries\" WHERE id = $2 RETURNING id, endpoint, event, status::text AS status, tenant_id, attempts, max_attempts, next_attempt_at, response_status, last_error, created_at, completed_at",
        [id.into(), source_id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let delivery = delivery_from_row(row)?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(delivery)))
}

async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let actor = state.auth.actor(&state.database, headers).await?.ok_or(ApiError::Unauthorized)?;
    if actor.has_role("admin") { Ok(()) } else { Err(ApiError::Forbidden) }
}

async fn require_admin_mutation(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let actor = state.auth.actor_for_mutation(&state.database, headers).await?
        .ok_or(ApiError::Unauthorized)?;
    if actor.has_role("admin") { Ok(()) } else { Err(ApiError::Forbidden) }
}

fn ensure_enabled() -> Result<(), ApiError> {
    if config::WEBHOOKS_ENABLED { Ok(()) } else { Err(ApiError::NotFound) }
}

async fn lock_status<C: ConnectionTrait>(database: &C, id: uuid::Uuid) -> Result<String, ApiError> {
    let row = database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT status::text AS status FROM \"_appstruct_webhook_deliveries\" WHERE id = $1 FOR UPDATE",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    Ok(row.try_get("", "status")?)
}

fn delivery_from_row(row: sea_orm::QueryResult) -> Result<AdminWebhookDelivery, sea_orm::DbErr> {
    Ok(AdminWebhookDelivery {
        id: row.try_get("", "id")?, endpoint: row.try_get("", "endpoint")?,
        event: row.try_get("", "event")?, status: row.try_get("", "status")?,
        tenant_id: row.try_get("", "tenant_id")?, attempts: row.try_get("", "attempts")?,
        max_attempts: row.try_get("", "max_attempts")?,
        next_attempt_at: row.try_get("", "next_attempt_at")?,
        response_status: row.try_get("", "response_status")?,
        last_error: row.try_get("", "last_error")?, created_at: row.try_get("", "created_at")?,
        completed_at: row.try_get("", "completed_at")?,
    })
}
