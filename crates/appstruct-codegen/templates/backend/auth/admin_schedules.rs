use super::admin::{require_admin, require_admin_mutation};
use super::config;
use crate::{ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::Serialize;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/schedules", get(list_schedules))
        .route("/api/admin/schedules/{id}/pause", post(pause_schedule))
        .route("/api/admin/schedules/{id}/resume", post(resume_schedule))
        .route("/api/admin/schedules/{id}/trigger", post(trigger_schedule))
}

#[derive(Serialize)]
struct AdminSchedule {
    id: uuid::Uuid,
    name: String,
    cron: String,
    interval_seconds: Option<i64>,
    queue: String,
    kind: String,
    enabled: bool,
    paused: bool,
    next_run_at: chrono::DateTime<chrono::Utc>,
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct AdminScheduleList { data: Vec<AdminSchedule> }

#[derive(Serialize)]
struct AdminScheduleTrigger { job_id: uuid::Uuid }

async fn list_schedules(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<Json<AdminScheduleList>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_enabled()?;
    let rows = state.database.query_all_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT id, name, cron, interval_seconds, queue, kind, enabled, paused, next_run_at, last_run_at, created_at FROM \"_appstruct_job_schedules\" ORDER BY enabled DESC, name".to_owned(),
    )).await?;
    let data = rows.into_iter().map(schedule_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminScheduleList { data }))
}

async fn pause_schedule(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminSchedule>, ApiError> {
    set_paused(state, headers, id, true).await
}

async fn resume_schedule(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminSchedule>, ApiError> {
    set_paused(state, headers, id, false).await
}

async fn set_paused(
    state: AppState, headers: HeaderMap, id: String, paused: bool,
) -> Result<Json<AdminSchedule>, ApiError> {
    require_admin_mutation(&state, &headers).await?;
    ensure_enabled()?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_job_schedules\" SET paused = $2 WHERE id = $1 AND enabled RETURNING id, name, cron, interval_seconds, queue, kind, enabled, paused, next_run_at, last_run_at, created_at",
        [id.into(), paused.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(schedule_from_row(row)?))
}

async fn trigger_schedule(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<(StatusCode, Json<AdminScheduleTrigger>), ApiError> {
    require_admin_mutation(&state, &headers).await?;
    ensure_enabled()?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT queue, kind, payload FROM \"_appstruct_job_schedules\" WHERE id = $1 AND enabled FOR UPDATE",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let queue: String = row.try_get("", "queue")?;
    let kind: String = row.try_get("", "kind")?;
    let payload: serde_json::Value = row.try_get("", "payload")?;
    let receipt = crate::jobs::enqueue(
        &transaction, &queue, &kind, &payload, None, None, None,
    ).await.map_err(|error| {
        tracing::error!(%error, schedule_id = %id, "manual schedule trigger failed");
        ApiError::Internal
    })?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(AdminScheduleTrigger { job_id: receipt.id })))
}

fn ensure_enabled() -> Result<(), ApiError> {
    if config::JOBS_ENABLED { Ok(()) } else { Err(ApiError::NotFound) }
}

fn schedule_from_row(row: sea_orm::QueryResult) -> Result<AdminSchedule, sea_orm::DbErr> {
    Ok(AdminSchedule {
        id: row.try_get("", "id")?, name: row.try_get("", "name")?,
        cron: row.try_get("", "cron")?, interval_seconds: row.try_get("", "interval_seconds")?,
        queue: row.try_get("", "queue")?, kind: row.try_get("", "kind")?,
        enabled: row.try_get("", "enabled")?, paused: row.try_get("", "paused")?,
        next_run_at: row.try_get("", "next_run_at")?, last_run_at: row.try_get("", "last_run_at")?,
        created_at: row.try_get("", "created_at")?,
    })
}
