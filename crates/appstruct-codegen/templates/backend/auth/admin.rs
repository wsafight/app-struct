use super::config;
use super::handlers::quote_ident;
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/overview", get(admin_overview))
        .route("/api/admin/users", get(list_admin_users))
        .route("/api/admin/users/{id}/revoke-sessions", post(revoke_admin_user_sessions))
        .route("/api/admin/jobs", get(list_admin_jobs))
        .route("/api/admin/jobs/{id}/retry", post(retry_admin_job))
        .route("/api/admin/jobs/{id}/replay", post(replay_admin_job))
}

#[derive(Serialize)]
struct AdminOverview {
    users: i64,
    organizations: i64,
    invitations: i64,
    sessions: i64,
    jobs_queued: i64,
    jobs_dead: i64,
    mail_deliveries: i64,
    files: i64,
    audit_events: i64,
}

#[derive(Serialize)]
struct AdminUser {
    id: uuid::Uuid,
    email: String,
    roles: Vec<String>,
    email_verified: bool,
    active_sessions: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct AdminUserList { data: Vec<AdminUser> }

#[derive(Serialize)]
struct AdminSessionRevocation { revoked: u64 }

#[derive(Deserialize)]
struct AdminUserQuery { limit: Option<u64> }

#[derive(Deserialize)]
struct AdminJobQuery { status: Option<String>, limit: Option<u64> }

#[derive(Serialize)]
struct AdminJob {
    id: uuid::Uuid,
    queue: String,
    kind: String,
    status: String,
    tenant_id: Option<uuid::Uuid>,
    attempts: i32,
    max_attempts: i32,
    run_at: chrono::DateTime<chrono::Utc>,
    last_error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct AdminJobList { data: Vec<AdminJob> }

async fn admin_overview(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<Json<AdminOverview>, ApiError> {
    require_admin(&state, &headers).await?;
    Ok(Json(AdminOverview {
        users: count_table(&state, config::USER_TABLE).await?,
        organizations: optional_count(&state, config::TENANT_ENABLED, "_appstruct_tenant_organizations").await?,
        invitations: optional_count(&state, config::TENANT_ENABLED, "_appstruct_tenant_invitations").await?,
        sessions: count_table(&state, "_appstruct_auth_sessions").await?,
        jobs_queued: optional_count_where(&state, config::JOBS_ENABLED, "_appstruct_jobs", "status = 'queued'").await?,
        jobs_dead: optional_count_where(&state, config::JOBS_ENABLED, "_appstruct_jobs", "status = 'dead'").await?,
        mail_deliveries: optional_count(&state, config::MAIL_ENABLED, "_appstruct_mail_deliveries").await?,
        files: optional_count(&state, config::FILE_ENABLED, "_appstruct_files").await?,
        audit_events: optional_count(&state, config::AUDIT_ENABLED, "_appstruct_audit_events").await?,
    }))
}

async fn list_admin_users(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<AdminUserQuery>,
) -> Result<Json<AdminUserList>, ApiError> {
    require_admin(&state, &headers).await?;
    let limit = input.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::InvalidQuery("limit must be between 1 and 100".to_owned()));
    }
    let sql = format!(
        "SELECT u.{id} AS id, u.{email} AS email, a.roles, a.email_verified_at, a.created_at, (SELECT COUNT(*) FROM \"_appstruct_auth_sessions\" s WHERE s.user_id = a.user_id AND s.revoked_at IS NULL AND s.expires_at > CURRENT_TIMESTAMP) AS active_sessions FROM {users} u JOIN \"_appstruct_auth_accounts\" a ON a.user_id = u.{id} ORDER BY a.created_at DESC, u.{id} DESC LIMIT $1",
        id = quote_ident(config::USER_ID_COLUMN),
        email = quote_ident(config::USER_EMAIL_COLUMN),
        users = quote_ident(config::USER_TABLE),
    );
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, sql, [i64::try_from(limit).unwrap_or(100).into()],
    )).await?;
    let data = rows.into_iter().map(admin_user_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminUserList { data }))
}

async fn revoke_admin_user_sessions(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminSessionRevocation>, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    require_admin(&state, &headers).await?;
    let user_id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let user_exists = format!(
        "SELECT 1 FROM \"_appstruct_auth_accounts\" WHERE user_id = $1 AND EXISTS (SELECT 1 FROM {users} WHERE {users}.{id} = $1)",
        users = quote_ident(config::USER_TABLE), id = quote_ident(config::USER_ID_COLUMN),
    );
    if state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, user_exists, [user_id.into()],
    )).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let result = state.database.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL",
        [user_id.into()],
    )).await?;
    Ok(Json(AdminSessionRevocation { revoked: result.rows_affected() }))
}

async fn list_admin_jobs(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<AdminJobQuery>,
) -> Result<Json<AdminJobList>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_jobs_enabled()?;
    if input.status.as_deref().is_some_and(|status| {
        !matches!(status, "queued" | "running" | "succeeded" | "dead")
    }) {
        return Err(ApiError::InvalidQuery(
            "status must be queued, running, succeeded, or dead".to_owned(),
        ));
    }
    let limit = input.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::InvalidQuery("limit must be between 1 and 100".to_owned()));
    }
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, queue, kind, status::text AS status, tenant_id, attempts, max_attempts, run_at, last_error, created_at, completed_at FROM \"_appstruct_jobs\" WHERE ($1::text IS NULL OR status::text = $1) ORDER BY created_at DESC, id DESC LIMIT $2",
        [input.status.into(), i64::try_from(limit).unwrap_or(100).into()],
    )).await?;
    let data = rows.into_iter().map(admin_job_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminJobList { data }))
}

async fn retry_admin_job(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminJob>, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    require_admin(&state, &headers).await?;
    ensure_jobs_enabled()?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    if lock_admin_job(&transaction, id).await? != "dead" {
        return Err(ApiError::Conflict("Only dead jobs can be retried".to_owned()));
    }
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_jobs\" SET status = 'queued', attempts = 0, run_at = CURRENT_TIMESTAMP, locked_by = NULL, locked_until = NULL, last_error = NULL, completed_at = NULL WHERE id = $1 RETURNING id, queue, kind, status::text AS status, tenant_id, attempts, max_attempts, run_at, last_error, created_at, completed_at",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let job = admin_job_from_row(row)?;
    transaction.commit().await?;
    Ok(Json(job))
}

async fn replay_admin_job(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<(StatusCode, Json<AdminJob>), ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    require_admin(&state, &headers).await?;
    ensure_jobs_enabled()?;
    let source_id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    let status = lock_admin_job(&transaction, source_id).await?;
    if !matches!(status.as_str(), "succeeded" | "dead") {
        return Err(ApiError::Conflict(
            "Only succeeded or dead jobs can be replayed".to_owned(),
        ));
    }
    let id = uuid::Uuid::now_v7();
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_jobs\" (id, queue, kind, payload, idempotency_key, tenant_id, status, attempts, max_attempts, backoff_seconds, run_at, created_at) SELECT $1, queue, kind, payload, NULL, tenant_id, 'queued', 0, max_attempts, backoff_seconds, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP FROM \"_appstruct_jobs\" WHERE id = $2 RETURNING id, queue, kind, status::text AS status, tenant_id, attempts, max_attempts, run_at, last_error, created_at, completed_at",
        [id.into(), source_id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let job = admin_job_from_row(row)?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(job)))
}

async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let actor = state.auth.actor(&state.database, headers).await?.ok_or(ApiError::Unauthorized)?;
    if actor.has_role("admin") { Ok(()) } else { Err(ApiError::Forbidden) }
}

fn ensure_jobs_enabled() -> Result<(), ApiError> {
    if config::JOBS_ENABLED { Ok(()) } else { Err(ApiError::NotFound) }
}

async fn lock_admin_job<C: ConnectionTrait>(database: &C, id: uuid::Uuid) -> Result<String, ApiError> {
    let row = database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT status::text AS status FROM \"_appstruct_jobs\" WHERE id = $1 FOR UPDATE",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    Ok(row.try_get("", "status")?)
}

fn admin_job_from_row(row: sea_orm::QueryResult) -> Result<AdminJob, sea_orm::DbErr> {
    Ok(AdminJob {
        id: row.try_get("", "id")?, queue: row.try_get("", "queue")?,
        kind: row.try_get("", "kind")?, status: row.try_get("", "status")?,
        tenant_id: row.try_get("", "tenant_id")?, attempts: row.try_get("", "attempts")?,
        max_attempts: row.try_get("", "max_attempts")?, run_at: row.try_get("", "run_at")?,
        last_error: row.try_get("", "last_error")?, created_at: row.try_get("", "created_at")?,
        completed_at: row.try_get("", "completed_at")?,
    })
}

fn admin_user_from_row(row: sea_orm::QueryResult) -> Result<AdminUser, sea_orm::DbErr> {
    let roles: serde_json::Value = row.try_get("", "roles")?;
    Ok(AdminUser {
        id: row.try_get("", "id")?, email: row.try_get("", "email")?,
        roles: serde_json::from_value(roles).map_err(|error| sea_orm::DbErr::Type(error.to_string()))?,
        email_verified: row.try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "email_verified_at")?.is_some(),
        active_sessions: row.try_get("", "active_sessions")?, created_at: row.try_get("", "created_at")?,
    })
}

async fn count_table(state: &AppState, table: &str) -> Result<i64, ApiError> {
    let sql = format!("SELECT COUNT(*) AS count FROM {}", quote_ident(table));
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, sql, [],
    )).await?.ok_or(ApiError::Internal)?;
    Ok(row.try_get("", "count")?)
}

async fn optional_count(state: &AppState, enabled: bool, table: &str) -> Result<i64, ApiError> {
    if enabled { count_table(state, table).await } else { Ok(0) }
}

async fn optional_count_where(
    state: &AppState, enabled: bool, table: &str, predicate: &str,
) -> Result<i64, ApiError> {
    if !enabled { return Ok(0); }
    let sql = format!("SELECT COUNT(*) AS count FROM {} WHERE {}", quote_ident(table), predicate);
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, sql, [],
    )).await?.ok_or(ApiError::Internal)?;
    Ok(row.try_get("", "count")?)
}
