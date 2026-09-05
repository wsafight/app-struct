use super::config;
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get, patch};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde::{Deserialize, Serialize};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/saved-views", get(list_views).post(create_view))
        .route("/api/saved-views/{id}", patch(update_view).delete(delete_view))
}

#[derive(Deserialize)]
struct ViewQuery { resource: String }

#[derive(Deserialize)]
struct CreateView {
    resource: String,
    name: String,
    query: String,
    visibility: String,
}

#[derive(Deserialize)]
struct UpdateView { name: String, query: String, visibility: String }

#[derive(Serialize)]
struct SavedView {
    id: uuid::Uuid,
    name: String,
    query: String,
    visibility: String,
    revision: i64,
    owned: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct SavedViewList { data: Vec<SavedView> }

struct ValidatedView { name: String, query: String, visibility: String }

async fn list_views(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<ViewQuery>,
) -> Result<Json<SavedViewList>, ApiError> {
    validate_resource(&input.resource)?;
    let context = state.context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.tenant();
    let scope = scope_key(tenant);
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, name, query, visibility, revision, owner_id = $2 AS owned, created_at, updated_at FROM \"_appstruct_saved_views\" WHERE resource = $1 AND scope_key = $3 AND (owner_id = $2 OR (visibility = 'team' AND $4::uuid IS NOT NULL AND tenant_id = $4)) ORDER BY owned DESC, updated_at DESC, id DESC",
        [input.resource.into(), actor.id.into(), scope.into(), tenant.into()],
    )).await?;
    let data = rows.into_iter().map(view_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(SavedViewList { data }))
}

async fn create_view(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<CreateView>,
) -> Result<(StatusCode, [(header::HeaderName, String); 1], Json<SavedView>), ApiError> {
    let resource = input.resource;
    validate_resource(&resource)?;
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.tenant();
    let input = validate_view(input.name, input.query, input.visibility, tenant)?;
    let id = uuid::Uuid::now_v7();
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_saved_views\" (id, owner_id, scope_key, tenant_id, resource, name, query, visibility, revision, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) RETURNING id, name, query, visibility, revision, TRUE AS owned, created_at, updated_at",
        [
            id.into(), actor.id.into(), scope_key(tenant).into(), tenant.into(),
            resource.into(), input.name.into(), input.query.into(), input.visibility.into(),
        ],
    )).await?.ok_or(ApiError::Internal)?;
    let view = view_from_row(row)?;
    Ok((StatusCode::CREATED, etag(view.revision), Json(view)))
}

async fn update_view(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
    Json(input): Json<UpdateView>,
) -> Result<([(header::HeaderName, String); 1], Json<SavedView>), ApiError> {
    let expected = expected_revision(&headers)?;
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.tenant();
    let input = validate_view(input.name, input.query, input.visibility, tenant)?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_saved_views\" SET name = $4, query = $5, visibility = $6, revision = revision + 1, updated_at = CURRENT_TIMESTAMP WHERE id = $1 AND owner_id = $2 AND scope_key = $3 AND revision = $7 RETURNING id, name, query, visibility, revision, TRUE AS owned, created_at, updated_at",
        [
            id.into(), actor.id.into(), scope_key(tenant).into(), input.name.into(),
            input.query.into(), input.visibility.into(), expected.into(),
        ],
    )).await?;
    let Some(row) = row else {
        return Err(missing_mutation(&state, id, actor.id, &scope_key(tenant), expected).await?);
    };
    let view = view_from_row(row)?;
    Ok((etag(view.revision), Json(view)))
}

async fn delete_view(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let expected = expected_revision(&headers)?;
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let scope = scope_key(context.tenant());
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let result = state.database.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "DELETE FROM \"_appstruct_saved_views\" WHERE id = $1 AND owner_id = $2 AND scope_key = $3 AND revision = $4",
        [id.into(), actor.id.into(), scope.clone().into(), expected.into()],
    )).await?;
    if result.rows_affected() == 1 { return Ok(StatusCode::NO_CONTENT); }
    Err(missing_mutation(&state, id, actor.id, &scope, expected).await?)
}

fn validate_view(
    name: String, query: String, visibility: String, tenant: Option<uuid::Uuid>,
) -> Result<ValidatedView, ApiError> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.len() > 80 {
        return Err(ApiError::InvalidQuery("view name must contain between 1 and 80 bytes".to_owned()));
    }
    if query.len() > 4_096 {
        return Err(ApiError::InvalidQuery("view query must not exceed 4096 bytes".to_owned()));
    }
    if !matches!(visibility.as_str(), "private" | "team") {
        return Err(ApiError::InvalidQuery("visibility must be private or team".to_owned()));
    }
    if visibility == "team" && (!config::TENANT_ENABLED || tenant.is_none()) {
        return Err(ApiError::InvalidQuery("team views require a tenant context".to_owned()));
    }
    Ok(ValidatedView { name, query, visibility })
}

fn validate_resource(resource: &str) -> Result<(), ApiError> {
    if config::SAVED_VIEW_RESOURCES.contains(&resource) { Ok(()) }
    else { Err(ApiError::InvalidQuery("unknown saved-view resource".to_owned())) }
}

fn expected_revision(headers: &HeaderMap) -> Result<i64, ApiError> {
    let value = headers.get(header::IF_MATCH).ok_or(ApiError::PreconditionRequired)?;
    let value = value.to_str().map_err(|_| ApiError::InvalidPrecondition)?;
    crate::parse_revision_etag(value).ok_or(ApiError::InvalidPrecondition)
}

fn etag(revision: i64) -> [(header::HeaderName, String); 1] {
    [(header::ETAG, crate::revision_etag(revision))]
}

fn scope_key(tenant: Option<uuid::Uuid>) -> String {
    tenant.map_or_else(|| "global".to_owned(), |id| id.to_string())
}

async fn missing_mutation(
    state: &AppState, id: uuid::Uuid, owner: uuid::Uuid, scope: &str, expected: i64,
) -> Result<ApiError, ApiError> {
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT revision FROM \"_appstruct_saved_views\" WHERE id = $1 AND owner_id = $2 AND scope_key = $3",
        [id.into(), owner.into(), scope.to_owned().into()],
    )).await?;
    Ok(match row {
        Some(row) if row.try_get::<i64>("", "revision")? != expected => ApiError::ConcurrentModification,
        _ => ApiError::NotFound,
    })
}

fn view_from_row(row: sea_orm::QueryResult) -> Result<SavedView, sea_orm::DbErr> {
    Ok(SavedView {
        id: row.try_get("", "id")?, name: row.try_get("", "name")?,
        query: row.try_get("", "query")?, visibility: row.try_get("", "visibility")?,
        revision: row.try_get("", "revision")?, owned: row.try_get("", "owned")?,
        created_at: row.try_get("", "created_at")?, updated_at: row.try_get("", "updated_at")?,
    })
}
