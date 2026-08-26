use crate::{ColumnSchema, DatabaseType};
use appstruct_ir::GeneratedValueIr;
use std::collections::BTreeSet;

pub(super) const fn expected_type(data_type: &DatabaseType) -> &'static str {
    match data_type {
        DatabaseType::Uuid => "uuid",
        DatabaseType::Text | DatabaseType::Enum { .. } => "text",
        DatabaseType::Integer => "integer",
        DatabaseType::Bigint => "bigint",
        DatabaseType::Decimal => "numeric",
        DatabaseType::Boolean => "boolean",
        DatabaseType::Date => "date",
        DatabaseType::Datetime => "timestamp with time zone",
        DatabaseType::Json => "jsonb",
    }
}

pub(super) fn expected_default(column: &ColumnSchema) -> Option<String> {
    match column.generated {
        Some(GeneratedValueIr::Now) => Some("current_timestamp".to_owned()),
        Some(GeneratedValueIr::Revision) => Some("1".to_owned()),
        Some(
            GeneratedValueIr::UuidV7 | GeneratedValueIr::AutoIncrement | GeneratedValueIr::Tenant,
        ) => None,
        None => column.default.as_deref().map(normalize_literal),
    }
}

pub(super) fn default(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let value = value.split("::").next().unwrap_or(value);
    Some(normalize_literal(trim_parentheses(value)))
}

pub(super) fn sql_literals(definition: &str) -> BTreeSet<String> {
    let mut output = BTreeSet::new();
    let mut characters = definition.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\'' {
            continue;
        }
        let mut literal = String::new();
        while let Some(character) = characters.next() {
            if character == '\'' {
                if characters.peek() == Some(&'\'') {
                    literal.push('\'');
                    characters.next();
                } else {
                    break;
                }
            } else {
                literal.push(character);
            }
        }
        output.insert(literal);
    }
    output
}

fn normalize_literal(value: &str) -> String {
    let value = value.trim();
    if let Some(quoted) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        return quoted.replace("''", "'");
    }
    match value.to_ascii_lowercase().as_str() {
        "now()" | "current_timestamp" => "current_timestamp".to_owned(),
        other => other.to_owned(),
    }
}

fn trim_parentheses(mut value: &str) -> &str {
    while value.starts_with('(') && value.ends_with(')') {
        value = value[1..value.len() - 1].trim();
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_postgres_defaults() {
        assert_eq!(default(Some("'draft'::text")), Some("draft".to_owned()));
        assert_eq!(default(Some("(1)::bigint")), Some("1".to_owned()));
        assert_eq!(default(Some("now()")), Some("current_timestamp".to_owned()));
    }

    #[test]
    fn extracts_enum_check_literals() {
        let values = sql_literals(
            "CHECK ((status = ANY (ARRAY['draft'::text, 'active'::text, 'it''s'::text])))",
        );
        assert_eq!(
            values,
            BTreeSet::from(["active".to_owned(), "draft".to_owned(), "it's".to_owned()])
        );
    }
}
