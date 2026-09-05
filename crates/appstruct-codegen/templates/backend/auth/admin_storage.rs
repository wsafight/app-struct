use super::admin::{AdminListMeta, admin_pagination, require_admin};
use super::config;
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde::{Deserialize, Serialize};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/mail", get(list_mail_deliveries))
        .route("/api/admin/mail/{id}", get(get_mail_delivery))
        .route("/api/admin/files", get(list_files))
        .route("/api/admin/files/{id}", get(get_file))
}

#[derive(Deserialize)]
struct StorageQuery {
    search: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[derive(Serialize)]
struct AdminMailSummary {
    id: uuid::Uuid,
    provider: String,
    template: String,
    sender: String,
    recipient: String,
    subject: String,
    tenant_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct AdminMailDelivery {
    #[serde(flatten)]
    summary: AdminMailSummary,
    text_body: String,
    html_body: Option<String>,
}

#[derive(Serialize)]
struct AdminMailList {
    data: Vec<AdminMailSummary>,
    meta: AdminListMeta,
}

#[derive(Serialize)]
struct AdminFile {
    id: uuid::Uuid,
    object_key: String,
    original_name: String,
    content_type: String,
    size: u64,
    checksum: String,
    tenant_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct AdminFileList {
    data: Vec<AdminFile>,
    meta: AdminListMeta,
    total_bytes: u64,
}

async fn list_mail_deliveries(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<StorageQuery>,
) -> Result<Json<AdminMailList>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_enabled(config::MAIL_ENABLED)?;
    let (page, page_size, offset) = admin_pagination(input.page, input.page_size)?;
    let search = normalized_search(input.search)?;
    let filter = "($1::text IS NULL OR recipient ILIKE '%' || $1 || '%' OR subject ILIKE '%' || $1 || '%' OR sender ILIKE '%' || $1 || '%' OR template ILIKE '%' || $1 || '%' OR provider ILIKE '%' || $1 || '%')";
    let count_sql = format!(
        "SELECT COUNT(*) AS total FROM \"_appstruct_mail_deliveries\" WHERE {filter}"
    );
    let total = query_i64(&state, &count_sql, search.clone()).await?;
    let list_sql = format!(
        "SELECT id, provider, template, sender, recipient, subject, tenant_id, created_at FROM \"_appstruct_mail_deliveries\" WHERE {filter} ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3"
    );
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, list_sql,
        [
            search.into(),
            i64::try_from(page_size).unwrap_or(100).into(),
            i64::try_from(offset).unwrap_or(i64::MAX).into(),
        ],
    )).await?;
    let data = rows.into_iter().map(mail_summary_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminMailList {
        data,
        meta: AdminListMeta { page, page_size, total: total.try_into().unwrap_or(0) },
    }))
}

async fn get_mail_delivery(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminMailDelivery>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_enabled(config::MAIL_ENABLED)?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, provider, template, sender, recipient, subject, tenant_id, created_at, text_body, html_body FROM \"_appstruct_mail_deliveries\" WHERE id = $1",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let text_body = row.try_get("", "text_body")?;
    let html_body = row.try_get("", "html_body")?;
    Ok(Json(AdminMailDelivery {
        summary: mail_summary_from_row(row)?, text_body, html_body,
    }))
}

async fn list_files(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<StorageQuery>,
) -> Result<Json<AdminFileList>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_enabled(config::FILE_ENABLED)?;
    let (page, page_size, offset) = admin_pagination(input.page, input.page_size)?;
    let search = normalized_search(input.search)?;
    let filter = "($1::text IS NULL OR object_key ILIKE '%' || $1 || '%' OR original_name ILIKE '%' || $1 || '%' OR content_type ILIKE '%' || $1 || '%' OR checksum ILIKE '%' || $1 || '%')";
    let totals_sql = format!(
        "SELECT COUNT(*) AS total, COALESCE(SUM(size), 0)::bigint AS total_bytes FROM \"_appstruct_files\" WHERE {filter}"
    );
    let totals = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, totals_sql, [search.clone().into()],
    )).await?.ok_or(ApiError::Internal)?;
    let total: i64 = totals.try_get("", "total")?;
    let total_bytes: i64 = totals.try_get("", "total_bytes")?;
    let list_sql = format!(
        "SELECT id, object_key, original_name, content_type, size, checksum, tenant_id, created_at FROM \"_appstruct_files\" WHERE {filter} ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3"
    );
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, list_sql,
        [
            search.into(),
            i64::try_from(page_size).unwrap_or(100).into(),
            i64::try_from(offset).unwrap_or(i64::MAX).into(),
        ],
    )).await?;
    let data = rows.into_iter().map(file_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminFileList {
        data,
        meta: AdminListMeta { page, page_size, total: total.try_into().unwrap_or(0) },
        total_bytes: total_bytes.try_into().unwrap_or(0),
    }))
}

async fn get_file(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<Json<AdminFile>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_enabled(config::FILE_ENABLED)?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, object_key, original_name, content_type, size, checksum, tenant_id, created_at FROM \"_appstruct_files\" WHERE id = $1",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(file_from_row(row)?))
}

fn normalized_search(search: Option<String>) -> Result<Option<String>, ApiError> {
    let search = search.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
    if search.as_ref().is_some_and(|value| value.len() > 200) {
        return Err(ApiError::InvalidQuery("search must not exceed 200 bytes".to_owned()));
    }
    Ok(search)
}

fn ensure_enabled(enabled: bool) -> Result<(), ApiError> {
    if enabled { Ok(()) } else { Err(ApiError::NotFound) }
}

async fn query_i64(
    state: &AppState, sql: &str, value: Option<String>,
) -> Result<i64, ApiError> {
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, sql.to_owned(), [value.into()],
    )).await?.ok_or(ApiError::Internal)?;
    Ok(row.try_get("", "total")?)
}

fn mail_summary_from_row(row: sea_orm::QueryResult) -> Result<AdminMailSummary, sea_orm::DbErr> {
    Ok(AdminMailSummary {
        id: row.try_get("", "id")?, provider: row.try_get("", "provider")?,
        template: row.try_get("", "template")?, sender: row.try_get("", "sender")?,
        recipient: row.try_get("", "recipient")?, subject: row.try_get("", "subject")?,
        tenant_id: row.try_get("", "tenant_id")?, created_at: row.try_get("", "created_at")?,
    })
}

fn file_from_row(row: sea_orm::QueryResult) -> Result<AdminFile, sea_orm::DbErr> {
    Ok(AdminFile {
        id: row.try_get("", "id")?, object_key: row.try_get("", "object_key")?,
        original_name: row.try_get("", "original_name")?, content_type: row.try_get("", "content_type")?,
        size: row.try_get::<i64>("", "size")?.try_into().unwrap_or(0),
        checksum: row.try_get("", "checksum")?, tenant_id: row.try_get("", "tenant_id")?,
        created_at: row.try_get("", "created_at")?,
    })
}
