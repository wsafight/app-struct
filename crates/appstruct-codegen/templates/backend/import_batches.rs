use crate::{ApiError, RequestContext, database::{DatabaseExecutor, Sql, Transaction}};
use axum::http::HeaderMap;
use sqlx::Row;

pub(crate) fn key(headers: &HeaderMap) -> Result<uuid::Uuid, ApiError> {
    headers.get("Idempotency-Key").and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .filter(|key| matches!(key.get_version_num(), 4 | 7))
        .ok_or_else(|| ApiError::InvalidQuery("CSV import requires a UUID v4/v7 Idempotency-Key".to_owned()))
}

fn scope(context: &RequestContext<'_>, resource: &str, key: uuid::Uuid) -> String {
    let actor = context.actor().map(|actor| actor.id);
    appstruct_runtime::sha256_hex(format!("{resource}\n{actor:?}\n{:?}\n{key}", context.tenant()))
}

pub(crate) async fn begin(
    transaction: &Transaction, context: &RequestContext<'_>, resource: &str, key: uuid::Uuid, body: &str,
) -> Result<Option<appstruct_runtime::BulkResult>, ApiError> {
    let scope = scope(context, resource, key);
    let digest = appstruct_runtime::sha256_hex(body);
    transaction.execute(Sql::bind(
        "INSERT INTO \"_appstruct_import_batches\" (scope, request_digest) VALUES ($1, $2) ON CONFLICT (scope) DO NOTHING",
        [scope.clone().into(), digest.clone().into()],
    )).await?;
    let row = transaction.fetch_optional(Sql::bind(
        "SELECT request_digest, result FROM \"_appstruct_import_batches\" WHERE scope = $1 FOR UPDATE", [scope.into()],
    )).await?.ok_or(ApiError::Internal)?;
    if row.try_get::<String, _>("request_digest")? != digest {
        return Err(ApiError::Conflict("The import batch was already used for different CSV content; create a new batch only after resolving the previous result".to_owned()));
    }
    row.try_get::<Option<serde_json::Value>, _>("result")?
        .map(|result| serde_json::from_value(result).map_err(|_| ApiError::Internal)).transpose()
}

pub(crate) async fn finish(
    transaction: &Transaction, context: &RequestContext<'_>, resource: &str, key: uuid::Uuid, result: &appstruct_runtime::BulkResult,
) -> Result<(), ApiError> {
    let value = serde_json::to_value(result).map_err(|_| ApiError::Internal)?;
    transaction.execute(Sql::bind(
        "UPDATE \"_appstruct_import_batches\" SET result = $2 WHERE scope = $1",
        [scope(context, resource, key).into(), value.into()],
    )).await?;
    Ok(())
}

pub(crate) async fn result(
    context: &RequestContext<'_>, resource: &str, key: uuid::Uuid,
) -> Result<appstruct_runtime::BulkResult, ApiError> {
    let row = context.database().fetch_optional(Sql::bind(
        "SELECT result FROM \"_appstruct_import_batches\" WHERE scope = $1", [scope(context, resource, key).into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let result = row.try_get::<Option<serde_json::Value>, _>("result")?.ok_or(ApiError::NotFound)?;
    serde_json::from_value(result).map_err(|_| ApiError::Internal)
}
