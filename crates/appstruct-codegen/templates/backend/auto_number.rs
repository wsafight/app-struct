use crate::{
    ApiError, RequestContext,
    database::{DatabaseExecutor, Sql},
};
use sqlx::Row;

#[allow(dead_code)]
pub(crate) async fn next_document_number(
    context: &RequestContext<'_>,
    table: &str,
    column: &str,
    prefix: &str,
) -> Result<String, ApiError> {
    let period = chrono::Utc::now().format("%Y%m").to_string();
    let tenant = context
        .tenant()
        .map_or_else(|| "global".to_owned(), |id| id.to_string());
    let scope = format!("{tenant}:{table}:{column}:{period}");
    let row = context
        .database()
        .fetch_optional(Sql::bind(
            "INSERT INTO \"_appstruct_number_sequences\" (scope, next_value, updated_at) VALUES ($1, 1, CURRENT_TIMESTAMP) ON CONFLICT (scope) DO UPDATE SET next_value = \"_appstruct_number_sequences\".next_value + 1, updated_at = CURRENT_TIMESTAMP RETURNING next_value",
            [scope.into()],
        ))
        .await?
        .ok_or(ApiError::Internal)?;
    let sequence: i64 = row.try_get("next_value")?;
    let clean_prefix: String = prefix
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect::<String>()
        .to_ascii_uppercase();
    let label = if clean_prefix.is_empty() {
        "DOC"
    } else {
        clean_prefix.as_str()
    };
    Ok(format!("{label}-{period}-{sequence:06}"))
}
