use crate::database::{DatabaseExecutor, Sql};
use crate::{ApiError, AppState, FileError, FileMetadata, RequestContext};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::Row as _;

#[derive(Deserialize)]
struct FileQuery {
    page: Option<u64>,
    page_size: Option<u64>,
}
#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}
#[derive(Serialize)]
struct FileList {
    data: Vec<FileMetadata>,
    meta: FileListMeta,
}
#[derive(Serialize)]
struct FileListMeta {
    page: u64,
    page_size: u64,
    total: u64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/files", get(list_files).post(upload_file))
        .route("/api/files/uploads", axum::routing::post(prepare_upload))
        .route("/api/files/uploads/{id}", get(upload_status).delete(cancel_upload))
        .route("/api/files/uploads/{id}/content", axum::routing::put(upload_content))
        .route("/api/files/uploads/{id}/complete", axum::routing::post(complete_upload))
        .route("/api/files/records/{resource}/{record_id}", get(list_attachments))
        .route("/api/files/records/{resource}/{record_id}/{file_id}", axum::routing::put(attach_file).delete(detach_file))
        .route("/api/files/records/{resource}/{record_id}/{file_id}/replace", axum::routing::put(replace_file))
        .route("/api/files/{id}", axum::routing::delete(delete_file))
        .route("/api/files/{id}/download", get(download_file))
        .layer(DefaultBodyLimit::max(__MAX_BYTES__))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareUpload { name: String, content_type: String, size: u64 }

#[derive(Serialize)]
struct UploadSession {
    id: uuid::Uuid,
    upload_url: String,
    direct: bool,
    expires_at: chrono::DateTime<chrono::Utc>,
}

async fn prepare_upload(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<PrepareUpload>) -> Result<(StatusCode, Json<UploadSession>), ApiError> {
    let context = state.mutation_context(&headers).await?;
    let scope = prefix(&context)?;
    state.file.validate_upload(&input.name, &input.content_type, input.size).map_err(file_error)?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let id = uuid::Uuid::now_v7();
    let key = format!("staging/{scope}{id}");
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);
    let signed = state.file.sign_upload(&key).await.map_err(file_error)?;
    let direct = signed.is_some();
    let upload_url = signed.unwrap_or_else(|| format!("/api/files/uploads/{id}/content"));
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", [format!("upload:{scope}:{}", actor.id).into()])).await?;
    let count = transaction.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM \"_appstruct_file_uploads\" WHERE actor_id = $1 AND expires_at > CURRENT_TIMESTAMP AND file_id IS NULL AND NOT cancelled", [actor.id.into()])).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    if count >= 20 { return Err(ApiError::Conflict("Too many pending uploads; finish or cancel an upload first".to_owned())); }
    transaction.execute(Sql::bind("INSERT INTO \"_appstruct_file_uploads\" (id, tenant_id, actor_id, object_key, original_name, content_type, size, expires_at, cancelled) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)", [id.into(), context.tenant().into(), actor.id.into(), key.into(), input.name.into(), input.content_type.into(), (input.size as i64).into(), expires_at.into()])).await?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(UploadSession { id, upload_url, direct, expires_at })))
}

async fn upload_row<C: DatabaseExecutor>(database: &C, context: &RequestContext<'_>, id: &str, locked: bool) -> Result<crate::database::PgRow, ApiError> {
    prefix(context)?;
    let id = uuid::Uuid::parse_str(id).map_err(|_| ApiError::InvalidId)?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let query = format!("SELECT * FROM \"_appstruct_file_uploads\" WHERE id = $1 AND actor_id = $2 AND tenant_id IS NOT DISTINCT FROM $3{}", if locked { " FOR UPDATE" } else { "" });
    database.fetch_optional(Sql::bind(query, [id.into(), actor.id.into(), context.tenant().into()])).await?.ok_or(ApiError::NotFound)
}

fn upload_pending(row: &crate::database::PgRow) -> Result<(), ApiError> {
    if row.try_get::<bool, _>("cancelled")? || row.try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at")? <= chrono::Utc::now() {
        return Err(ApiError::Conflict("Upload session expired or cancelled; start a new upload".to_owned()));
    }
    Ok(())
}

async fn upload_status(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let context = state.context(&headers).await?;
    let row = upload_row(&state.database, &context, &id, false).await?;
    Ok(Json(serde_json::json!({ "id": id, "file_id": row.try_get::<Option<uuid::Uuid>, _>("file_id")?, "expires_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at")?, "cancelled": row.try_get::<bool, _>("cancelled")? })))
}

async fn upload_content(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>, body: Bytes) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let row = upload_row(&state.database, &context, &id, false).await?;
    upload_pending(&row)?;
    if row.try_get::<Option<uuid::Uuid>, _>("file_id")?.is_some() { return Err(ApiError::Conflict("Upload is already complete".to_owned())); }
    if body.len() as i64 != row.try_get::<i64, _>("size")? { return Err(ApiError::InvalidQuery("Upload size differs from the declared size".to_owned())); }
    let key = row.try_get::<String, _>("object_key")?;
    state.file.put_staged_owned(&key, body).await.map_err(file_error)?;
    let current = upload_row(&state.database, &context, &id, false).await?;
    let pending = upload_pending(&current).and_then(|()| {
        if current.try_get::<Option<uuid::Uuid>, _>("file_id")?.is_some() {
            Err(ApiError::Conflict("Upload is already complete".to_owned()))
        } else { Ok(()) }
    });
    if pending.is_err() {
        if let Err(error) = state.file.delete_staged(&key).await { tracing::warn!(%error, "staged upload cleanup deferred to reconciliation"); }
    }
    pending?;
    Ok(StatusCode::NO_CONTENT)
}

async fn complete_upload(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<FileMetadata>, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let row = upload_row(&state.database, &context, &id, false).await?;
    if let Some(metadata) = completed_metadata(&state, &context, &row).await? { return Ok(Json(metadata)); }
    upload_pending(&row)?;
    let staged = row.try_get::<String, _>("object_key")?;
    let content = match state.file.staged_bytes(&staged).await {
        Ok(content) => content,
        Err(error) => {
            let latest = upload_row(&state.database, &context, &id, false).await?;
            if let Some(metadata) = completed_metadata(&state, &context, &latest).await? { return Ok(Json(metadata)); }
            return Err(file_error(error));
        }
    };
    if content.len() as i64 != row.try_get::<i64, _>("size")? { return Err(ApiError::InvalidQuery("Upload size differs from the declared size".to_owned())); }
    let key = format!("{}{}", prefix(&context)?, uuid::Uuid::now_v7());
    let prepared = state.file.prepare_owned(&key, &row.try_get::<String, _>("original_name")?, &row.try_get::<String, _>("content_type")?, Bytes::from(content)).await.map_err(file_error)?;
    let mut commit_started = false;
    let result: Result<FileMetadata, ApiError> = async {
        let transaction = state.database.begin().await?;
        let current = upload_row(&transaction, &context, &id, true).await?;
        if let Some(metadata) = completed_metadata(&state, &context, &current).await? { return Ok(metadata); }
        upload_pending(&current)?;
        let metadata = state.file.publish_prepared(&transaction, &prepared, context.tenant()).await.map_err(file_error)?;
        state.file.retain_with_connection(&transaction, &key, "appstruct::file::library", &metadata.id.to_string(), context.tenant()).await.map_err(file_error)?;
        transaction.execute(Sql::bind("UPDATE \"_appstruct_file_uploads\" SET file_id = $2 WHERE id = $1", [current.try_get::<uuid::Uuid, _>("id")?.into(), metadata.id.into()])).await?;
        commit_started = true;
        transaction.commit().await?;
        Ok(metadata)
    }.await;
    if !commit_started {
        if let Err(error) = state.file.delete_staged(&key).await { tracing::warn!(%error, "unpublished upload cleanup deferred to reconciliation"); }
    }
    if result.is_ok() {
        if let Err(error) = state.file.delete_staged(&staged).await { tracing::warn!(%error, "staged upload cleanup deferred to reconciliation"); }
    }
    result.map(Json)
}

async fn completed_metadata(state: &AppState, context: &RequestContext<'_>, row: &crate::database::PgRow) -> Result<Option<FileMetadata>, ApiError> {
    let Some(file_id) = row.try_get::<Option<uuid::Uuid>, _>("file_id")? else { return Ok(None); };
    let key = owned_key(state, context, &file_id.to_string()).await?;
    state.file.metadata(&key, context.tenant()).await.map(Some).map_err(file_error)
}

async fn cancel_upload(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let transaction = state.database.begin().await?;
    let row = upload_row(&transaction, &context, &id, true).await?;
    if row.try_get::<Option<uuid::Uuid>, _>("file_id")?.is_some() { return Err(ApiError::Conflict("Upload is complete; delete the file instead".to_owned())); }
    transaction.execute(Sql::bind("UPDATE \"_appstruct_file_uploads\" SET cancelled = true WHERE id = $1", [row.try_get::<uuid::Uuid, _>("id")?.into()])).await?;
    transaction.commit().await?;
    if let Err(error) = state.file.delete_staged(&row.try_get::<String, _>("object_key")?).await { tracing::warn!(%error, "cancelled upload cleanup deferred to reconciliation"); }
    Ok(StatusCode::NO_CONTENT)
}

async fn authorize_record(state: &AppState, context: &RequestContext<'_>, resource: &str, record_id: &str, write: bool) -> Result<(), ApiError> {
    prefix(context)?;
    match resource { __RESOURCE_ARMS__, _ => Err(ApiError::NotFound) }
}

fn canonical_record(resource: &str, record_id: &str) -> Result<String, ApiError> {
    match resource { __CANONICAL_ARMS__, _ => Err(ApiError::NotFound) }
}

async fn list_attachments(State(state): State<AppState>, headers: HeaderMap, Path((resource, record_id)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    let record_id = canonical_record(&resource, &record_id)?;
    let context = state.context(&headers).await?;
    authorize_record(&state, &context, &resource, &record_id, false).await?;
    let owner = serde_json::json!([{ "entity": format!("attachment:{resource}"), "record_id": record_id }]);
    let pattern = format!("{}%", prefix(&context)?);
    let rows = state.database.fetch_all(Sql::bind("SELECT id, original_name, content_type, size FROM \"_appstruct_files\" WHERE tenant_id IS NOT DISTINCT FROM $1 AND reference_owners @> $2::jsonb AND object_key LIKE $3 AND NOT deleting ORDER BY created_at, id LIMIT 100", [context.tenant().into(), owner.into(), pattern.into()])).await?;
    let data = rows.iter().map(|row| Ok(serde_json::json!({ "id": row.try_get::<uuid::Uuid, _>("id")?, "original_name": row.try_get::<String, _>("original_name")?, "content_type": row.try_get::<String, _>("content_type")?, "size": row.try_get::<i64, _>("size")? }))).collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(serde_json::json!({ "data": data })))
}

async fn attach_file(State(state): State<AppState>, headers: HeaderMap, Path((resource, record_id, file_id)): Path<(String, String, String)>) -> Result<StatusCode, ApiError> {
    let record_id = canonical_record(&resource, &record_id)?;
    let context = state.mutation_context(&headers).await?;
    authorize_record(&state, &context, &resource, &record_id, true).await?;
    let key = owned_key(&state, &context, &file_id).await?;
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", [format!("attachment:{}:{resource}:{record_id}", prefix(&context)?).into()])).await?;
    let owner = serde_json::json!([{ "entity": format!("attachment:{resource}"), "record_id": record_id }]);
    let count = transaction.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM \"_appstruct_files\" WHERE tenant_id IS NOT DISTINCT FROM $1 AND reference_owners @> $2::jsonb AND object_key LIKE $3 AND object_key <> $4 AND NOT deleting", [context.tenant().into(), owner.into(), format!("{}%", prefix(&context)?).into(), key.clone().into()])).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    if count >= 100 { return Err(ApiError::Conflict("A record can contain at most 100 attachments in this file scope".to_owned())); }
    state.file.retain_with_connection(&transaction, &key, &format!("attachment:{resource}"), &record_id, context.tenant()).await.map_err(file_error)?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replacement { replacement_file_id: uuid::Uuid }

async fn replace_file(State(state): State<AppState>, headers: HeaderMap, Path((resource, record_id, file_id)): Path<(String, String, String)>, Json(input): Json<Replacement>) -> Result<StatusCode, ApiError> {
    let record_id = canonical_record(&resource, &record_id)?;
    let context = state.mutation_context(&headers).await?;
    authorize_record(&state, &context, &resource, &record_id, true).await?;
    let old = owned_key(&state, &context, &file_id).await?;
    let new = owned_key(&state, &context, &input.replacement_file_id.to_string()).await?;
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", [format!("attachment:{}:{resource}:{record_id}", prefix(&context)?).into()])).await?;
    let owner = serde_json::json!([{ "entity": format!("attachment:{resource}"), "record_id": record_id }]);
    let existing = transaction.fetch_optional(Sql::bind("SELECT 1 FROM \"_appstruct_files\" WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2 AND reference_owners @> $3::jsonb AND NOT deleting", [old.clone().into(), context.tenant().into(), owner.clone().into()])).await?;
    if existing.is_none() {
        if transaction.fetch_optional(Sql::bind("SELECT 1 FROM \"_appstruct_files\" WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2 AND reference_owners @> $3::jsonb AND NOT deleting", [new.into(), context.tenant().into(), owner.into()])).await?.is_some() { return Ok(StatusCode::NO_CONTENT); }
        return Err(ApiError::NotFound);
    }
    if old != new {
        state.file.retain_with_connection(&transaction, &new, &format!("attachment:{resource}"), &record_id, context.tenant()).await.map_err(file_error)?;
        state.file.release_with_connection(&transaction, &old, &format!("attachment:{resource}"), &record_id, context.tenant()).await.map_err(file_error)?;
    }
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn detach_file(State(state): State<AppState>, headers: HeaderMap, Path((resource, record_id, file_id)): Path<(String, String, String)>) -> Result<StatusCode, ApiError> {
    let record_id = canonical_record(&resource, &record_id)?;
    let context = state.mutation_context(&headers).await?;
    authorize_record(&state, &context, &resource, &record_id, true).await?;
    let key = owned_key(&state, &context, &file_id).await?;
    state.file.release(&key, &format!("attachment:{resource}"), &record_id, context.tenant()).await.map_err(file_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn prefix(context: &RequestContext<'_>) -> Result<String, ApiError> {
    context.actor().ok_or(ApiError::Unauthorized)?;
    let scope = __SCOPE__;
    Ok(format!("uploads/{scope}/"))
}

async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<FileQuery>,
) -> Result<Json<FileList>, ApiError> {
    let context = state.context(&headers).await?;
    let pattern = format!("{}%", prefix(&context)?);
    let page = input.page.unwrap_or(1);
    let page_size = input.page_size.unwrap_or(25);
    if !(1..=10_000).contains(&page) || !(1..=100).contains(&page_size) {
        return Err(ApiError::InvalidQuery("invalid file pagination".to_owned()));
    }
    let rows = state.database.fetch_all(Sql::bind(
        "SELECT id, object_key, original_name, content_type, size, checksum, tenant_id, created_at FROM \"_appstruct_files\" WHERE object_key LIKE $1 AND tenant_id IS NOT DISTINCT FROM $2 AND NOT deleting ORDER BY created_at DESC, id DESC LIMIT $3 OFFSET $4",
        [pattern.clone().into(), context.tenant().into(), (page_size as i64).into(), (((page - 1) * page_size) as i64).into()],
    )).await?;
    let count = state.database.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM \"_appstruct_files\" WHERE object_key LIKE $1 AND tenant_id IS NOT DISTINCT FROM $2 AND NOT deleting", [pattern.into(), context.tenant().into()])).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    let mut data = Vec::new();
    for row in rows {
        data.push(FileMetadata {
            id: row.try_get("id")?,
            object_key: row.try_get("object_key")?,
            original_name: row.try_get("original_name")?,
            content_type: row.try_get("content_type")?,
            size: u64::try_from(row.try_get::<i64, _>("size")?).unwrap_or(0),
            checksum: row.try_get("checksum")?,
            tenant_id: row.try_get("tenant_id")?,
            created_at: row.try_get("created_at")?,
        });
    }
    Ok(Json(FileList {
        data,
        meta: FileListMeta {
            page,
            page_size,
            total: u64::try_from(count).unwrap_or(0),
        },
    }))
}

async fn upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<UploadQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<FileMetadata>), ApiError> {
    let context = state.mutation_context(&headers).await?;
    let key = format!("{}{}", prefix(&context)?, uuid::Uuid::now_v7());
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream");
    let prepared = state.file.prepare_owned(&key, &input.name, content_type, body).await.map_err(file_error)?;
    let transaction = state.database.begin().await?;
    let metadata = state.file.publish_prepared(&transaction, &prepared, context.tenant()).await.map_err(file_error)?;
    state
        .file
        .retain_with_connection(
            &transaction,
            &key,
            "appstruct::file::library",
            &metadata.id.to_string(),
            context.tenant(),
        )
        .await
        .map_err(file_error)?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(metadata)))
}

async fn owned_key(
    state: &AppState,
    context: &RequestContext<'_>,
    id: &str,
) -> Result<String, ApiError> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| ApiError::InvalidId)?;
    let pattern = format!("{}%", prefix(context)?);
    let row = state.database.fetch_optional(Sql::bind("SELECT object_key FROM \"_appstruct_files\" WHERE id = $1 AND object_key LIKE $2 AND tenant_id IS NOT DISTINCT FROM $3 AND NOT deleting", [id.into(), pattern.into(), context.tenant().into()])).await?.ok_or(ApiError::NotFound)?;
    Ok(row.try_get("object_key")?)
}

async fn download_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let context = state.context(&headers).await?;
    let key = owned_key(&state, &context, &id).await?;
    let (metadata, bytes) = state
        .file
        .stream(&key, context.tenant())
        .await
        .map_err(file_error)?;
    let mut response = Response::new(Body::from_stream(bytes));
    response.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from_str(&metadata.size.to_string()).map_err(|_| ApiError::Internal)?);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&metadata.content_type).map_err(|_| ApiError::Internal)?,
    );
    let filename = metadata
        .original_name
        .bytes()
        .map(|byte| format!("%{byte:02X}"))
        .collect::<String>();
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"download\"; filename*=UTF-8''{filename}"
        ))
        .map_err(|_| ApiError::Internal)?,
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

async fn delete_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = uuid::Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidId)?
        .to_string();
    let context = state.mutation_context(&headers).await?;
    let key = owned_key(&state, &context, &id).await?;
    let transaction = state.database.begin().await?;
    let row = transaction.fetch_optional(Sql::bind("SELECT reference_owners FROM \"_appstruct_files\" WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2 FOR UPDATE", [key.clone().into(), context.tenant().into()])).await?.ok_or(ApiError::NotFound)?;
    let owners: serde_json::Value = row.try_get("reference_owners")?;
    for owner in owners.as_array().ok_or(ApiError::Internal)? {
        if let (Some(entity), Some(record_id)) = (owner["entity"].as_str(), owner["record_id"].as_str()) {
            if let Some(resource) = entity.strip_prefix("attachment:") {
                if !record_exists(&transaction, resource, record_id, context.tenant()).await? {
                    state.file.release_with_connection(&transaction, &key, entity, record_id, context.tenant()).await.map_err(file_error)?;
                }
            }
        }
    }
    state.file.release_with_connection(&transaction, &key, "appstruct::file::library", &id, context.tenant()).await.map_err(file_error)?;
    state.file.delete_with_connection(&transaction, &key, context.tenant()).await.map_err(file_error)?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn record_exists<C: DatabaseExecutor>(database: &C, resource: &str, record_id: &str, tenant: Option<uuid::Uuid>) -> Result<bool, ApiError> {
    Ok(match resource { __EXISTENCE_ARMS__, _ => true })
}

fn file_error(error: FileError) -> ApiError {
    match error {
        FileError::InvalidName | FileError::InvalidContentType | FileError::TooLarge { .. } => {
            ApiError::InvalidQuery(error.to_string())
        }
        FileError::ReferenceConflict => {
            ApiError::Conflict("The file is still referenced by business data".to_owned())
        }
        FileError::Database(error) => ApiError::Database(error),
        other => {
            tracing::error!(%other, "file library operation failed");
            ApiError::Internal
        }
    }
}
