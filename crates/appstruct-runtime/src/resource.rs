// Shared resource protocol types and parsing helpers.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<u64>,
    pub page_size: Option<u64>,
    pub cursor: Option<String>,
    pub limit: Option<u64>,
    pub sort: Option<String>,
    pub q: Option<String>,
    #[serde(flatten)]
    pub filters: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ListMeta {
    Page {
        page: u64,
        page_size: u64,
        total: u64,
    },
    Cursor {
        limit: u64,
        next_cursor: Option<String>,
        has_more: bool,
    },
}

#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub meta: ListMeta,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BulkUpdateInput<T> {
    pub ids: Vec<String>,
    pub patch: T,
    pub expected_revisions: BTreeMap<String, i64>,
}

#[derive(Debug, Deserialize)]
pub struct BulkDeleteInput {
    pub ids: Vec<String>,
    pub expected_revisions: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct BulkFailure {
    pub id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct BulkResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<BulkFailure>,
}

#[must_use]
pub fn bulk_failure(id: &str, code: &str, message: impl Into<String>) -> BulkFailure {
    BulkFailure {
        id: id.to_owned(),
        code: code.to_owned(),
        message: message.into(),
    }
}

#[must_use]
pub fn encode_cursor(value: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("v1:{value}"))
}

#[must_use]
pub fn decode_cursor(cursor: &str) -> Option<String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|value| value.strip_prefix("v1:").map(str::to_owned))
        .filter(|value| !value.is_empty())
}

#[must_use]
pub fn parse_revision_etag(value: &str) -> Option<i64> {
    value
        .strip_prefix("\"rev-")
        .and_then(|value| value.strip_suffix('"'))
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= 1)
}

#[must_use]
pub fn revision_etag(revision: i64) -> String {
    format!("\"rev-{revision}\"")
}

#[must_use]
pub fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsvError;

impl fmt::Display for CsvError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CSV contains an unterminated quote")
    }
}

impl Error for CsvError {}

/// Parses CSV input into rows and decoded field values.
///
/// # Errors
///
/// Returns [`CsvError`] when a quoted field is not terminated.
pub fn parse_csv_rows(body: &str) -> Result<Vec<Vec<String>>, CsvError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                value.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut value)),
            '\n' if !quoted => {
                row.push(std::mem::take(&mut value));
                if !row.iter().all(String::is_empty) {
                    rows.push(std::mem::take(&mut row));
                }
            }
            '\r' if !quoted => {}
            _ => value.push(character),
        }
    }
    if quoted {
        return Err(CsvError);
    }
    if !value.is_empty() || !row.is_empty() {
        row.push(value);
        if !row.iter().all(String::is_empty) {
            rows.push(row);
        }
    }
    Ok(rows)
}

#[must_use]
pub fn csv_json_value(value: &str, kind: &str) -> serde_json::Value {
    if value.is_empty() {
        return serde_json::Value::Null;
    }
    match kind {
        "boolean" => value.parse::<bool>().map_or_else(
            |_| serde_json::Value::String(value.to_owned()),
            serde_json::Value::Bool,
        ),
        "integer" => value.parse::<i32>().map_or_else(
            |_| serde_json::Value::String(value.to_owned()),
            |value| serde_json::json!(value),
        ),
        "bigint" => value.parse::<i64>().map_or_else(
            |_| serde_json::Value::String(value.to_owned()),
            |value| serde_json::json!(value),
        ),
        _ => serde_json::Value::String(value.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_and_rejects_other_versions() {
        let cursor = encode_cursor("018f");
        assert_eq!(decode_cursor(&cursor).as_deref(), Some("018f"));
        let wrong_version = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("v2:018f");
        assert_eq!(decode_cursor(&wrong_version), None);
    }

    #[test]
    fn revision_etags_are_strict() {
        assert_eq!(parse_revision_etag(&revision_etag(7)), Some(7));
        assert_eq!(parse_revision_etag("rev-7"), None);
        assert_eq!(parse_revision_etag("\"rev-0\""), None);
    }

    #[test]
    fn csv_parser_handles_escaped_quotes_and_newlines() {
        let rows = parse_csv_rows("name,note\nAda,\"one, \"\"two\"\"\"\n").unwrap();
        assert_eq!(rows[1], ["Ada", "one, \"two\""]);
        assert!(parse_csv_rows("name\n\"unterminated").is_err());
    }
}
