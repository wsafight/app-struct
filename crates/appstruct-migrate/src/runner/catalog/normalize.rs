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
                .is_none_or(|next| !next.is_ascii_alphanumeric() && !matches!(*next, b'_' | b'['))
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
    fn normalizes_postgres_partial_index_predicate_formatting() {
        for (input, expected) in [
            (" status = 'running' ", "status = 'running'"),
            ("(status = 'running'::text)", "status = 'running'"),
            ("(((status = 'running'::TEXT)))", "status = 'running'"),
            ("(status::text = 'running'::text)", "status = 'running'"),
            ("((deleted_at IS NULL))", "deleted_at IS NULL"),
            (
                "(status = 'it''s (ready)'::TEXT)",
                "status = 'it''s (ready)'",
            ),
            ("(state = 'ready'::text)", "state = 'ready'"),
        ] {
            assert_eq!(
                index_predicate(Some(input)).as_deref(),
                Some(expected),
                "predicate: {input}"
            );
        }
        assert_eq!(index_predicate(None), None);
    }

    #[test]
    fn predicate_normalization_preserves_sql_semantics() {
        for (input, expected) in [
            ("(first) OR (second)", "(first) OR (second)"),
            (
                "(\"status::text\" = 'queued::text')",
                "\"status::text\" = 'queued::text'",
            ),
            ("(payload = '1'::jsonb)", "payload = '1'::jsonb"),
            ("(kind = 'x'::textual)", "kind = 'x'::textual"),
            ("(tags = '{}'::text[])", "tags = '{}'::text[]"),
            ("((status = 'queued')", "((status = 'queued')"),
        ] {
            assert_eq!(
                index_predicate(Some(input)).as_deref(),
                Some(expected),
                "predicate: {input}"
            );
        }
    }

    #[test]
    fn predicate_normalization_handles_quoted_and_nested_content() {
        for (input, expected) in [
            ("(lower(status) = 'ready'::text)", "lower(status) = 'ready'"),
            (
                "(\"schema\".\"status\" = 'queued'::text)",
                "\"schema\".\"status\" = 'queued'",
            ),
            (
                r"(status = E'it\'s (ready)'::text)",
                r"status = E'it\'s (ready)'",
            ),
            ("(状态 = '就绪'::text)", "状态 = '就绪'"),
        ] {
            assert_eq!(
                index_predicate(Some(input)).as_deref(),
                Some(expected),
                "predicate: {input}"
            );
        }
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

    #[test]
    fn maps_expected_column_types_and_generated_defaults() {
        assert_eq!(expected_type(&DatabaseType::Uuid), "uuid");
        assert_eq!(expected_type(&DatabaseType::Text), "text");
        assert_eq!(
            expected_type(&DatabaseType::Enum {
                values: vec!["a".to_owned()]
            }),
            "text"
        );
        assert_eq!(expected_type(&DatabaseType::Integer), "integer");
        assert_eq!(expected_type(&DatabaseType::Bigint), "bigint");
        assert_eq!(expected_type(&DatabaseType::Decimal), "numeric");
        assert_eq!(expected_type(&DatabaseType::Boolean), "boolean");
        assert_eq!(expected_type(&DatabaseType::Date), "date");
        assert_eq!(
            expected_type(&DatabaseType::Datetime),
            "timestamp with time zone"
        );
        assert_eq!(expected_type(&DatabaseType::Json), "jsonb");

        let now = ColumnSchema {
            id: "created".to_owned(),
            name: "created".to_owned(),
            data_type: DatabaseType::Datetime,
            nullable: false,
            primary_key: false,
            unique: false,
            default: None,
            generated: Some(GeneratedValueIr::Now),
        };
        assert_eq!(expected_default(&now).as_deref(), Some("current_timestamp"));
        let mut revision = now.clone();
        revision.generated = Some(GeneratedValueIr::Revision);
        assert_eq!(expected_default(&revision).as_deref(), Some("1"));
        for generated in [
            GeneratedValueIr::UuidV7,
            GeneratedValueIr::AutoIncrement,
            GeneratedValueIr::Tenant,
        ] {
            let mut column = now.clone();
            column.generated = Some(generated);
            assert_eq!(expected_default(&column), None);
        }
        let mut literal = now;
        literal.generated = None;
        literal.default = Some("'draft'".to_owned());
        assert_eq!(expected_default(&literal).as_deref(), Some("draft"));
        assert_eq!(default(None), None);
        assert_eq!(
            default(Some("CURRENT_TIMESTAMP")),
            Some("current_timestamp".to_owned())
        );
    }
}
