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

pub(super) fn index_predicate(value: Option<&str>) -> Option<String> {
    let value = trim_parentheses(value?.trim());
    Some(remove_postgres_text_casts(value))
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
    while outer_parentheses_enclose(value) {
        value = value[1..value.len() - 1].trim();
    }
    value
}

fn outer_parentheses_enclose(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return false;
    }
    let mut depth = 0_usize;
    let mut quote = None;
    let mut index = 0_usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                    continue;
                }
                quote = None;
            } else if byte == b'\\' {
                index += 2;
                continue;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && index + 1 != bytes.len() {
                        return false;
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    depth == 0 && quote.is_none()
}

fn remove_postgres_text_casts(value: &str) -> String {
    const TEXT_CAST: &[u8] = b"::text";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut copied_from = 0_usize;
    let mut quote = None;
    let mut index = 0_usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                    continue;
                }
                quote = None;
            } else if byte == b'\\' {
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        let cast_end = index.saturating_add(TEXT_CAST.len());
        if bytes
            .get(index..cast_end)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(TEXT_CAST))
            && bytes
                .get(cast_end)
                .is_none_or(|next| !next.is_ascii_alphanumeric() && *next != b'_')
        {
            output.push_str(&value[copied_from..index]);
            copied_from = cast_end;
            index = cast_end;
            continue;
        }
        index += 1;
    }
    output.push_str(&value[copied_from..]);
    output
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
    fn normalizes_postgres_partial_index_predicates() {
        assert_eq!(
            index_predicate(Some("status = 'running'")),
            index_predicate(Some("(status = 'running'::text)"))
        );
        assert_eq!(
            index_predicate(Some("deleted_at IS NULL")),
            index_predicate(Some("((deleted_at IS NULL))"))
        );
        assert_eq!(
            index_predicate(Some("(status = 'it''s (ready)'::TEXT)")),
            Some("status = 'it''s (ready)'".to_owned())
        );
        assert_eq!(
            index_predicate(Some("(first) OR (second)")),
            Some("(first) OR (second)".to_owned())
        );
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
