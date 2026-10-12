// Shared list-query bounds and LIKE pattern escaping.

use serde::{Deserialize, Serialize};

/// A bounded, field-based filter tree. Values use the same textual codecs as query parameters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum FilterExpression {
    All {
        all: Vec<Self>,
    },
    Any {
        any: Vec<Self>,
    },
    Condition {
        field: String,
        op: FilterOperator,
        #[serde(default)]
        values: Vec<String>,
    },
}

/// Operators exposed by generated resource queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    NotIn,
    IsNull,
    NotNull,
    Contains,
    StartsWith,
    EndsWith,
    JsonContains,
    JsonContainedBy,
    JsonKey,
}

impl FilterOperator {
    /// The corresponding sift-query operator name.
    #[must_use]
    pub const fn sift_name(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Gt => "gt",
            Self::Gte => "ge",
            Self::Lt => "lt",
            Self::Lte => "le",
            Self::In => "il",
            Self::NotIn => "ni",
            Self::IsNull => "nl",
            Self::NotNull => "nn",
            Self::Contains => "ct",
            Self::StartsWith => "sw",
            Self::EndsWith => "ew",
            Self::JsonContains => "json_contains",
            Self::JsonContainedBy => "json_contained_by",
            Self::JsonKey => "json_key",
        }
    }
}

impl FilterExpression {
    /// Parses and validates depth, node, value and byte budgets before building SQL.
    ///
    /// # Errors
    /// Returns a diagnostic for malformed or overly complex filter trees.
    pub fn parse(source: &str) -> Result<Self, String> {
        if source.len() > 16_384 {
            return Err("`where` must not exceed 16384 bytes".to_owned());
        }
        let expression: Self = serde_json::from_str(source)
            .map_err(|_| "invalid `where` filter expression".to_owned())?;
        let mut nodes = 0;
        let mut values = 0;
        expression.validate(1, &mut nodes, &mut values)?;
        Ok(expression)
    }

    fn validate(&self, depth: usize, nodes: &mut usize, values: &mut usize) -> Result<(), String> {
        *nodes += 1;
        if depth > 8 || *nodes > 64 {
            return Err("filter tree exceeds its depth or node budget".to_owned());
        }
        match self {
            Self::All { all: children } | Self::Any { any: children } => {
                if children.is_empty() {
                    return Err("filter groups must not be empty".to_owned());
                }
                for child in children {
                    child.validate(depth + 1, nodes, values)?;
                }
            }
            Self::Condition {
                field,
                op,
                values: inputs,
            } => {
                if field.is_empty() || field.len() > 64 || !crate::is_rust_field_name(field) {
                    return Err("filter field must be a public scalar field name".to_owned());
                }
                let valid = match op {
                    FilterOperator::IsNull | FilterOperator::NotNull => inputs.is_empty(),
                    FilterOperator::In | FilterOperator::NotIn => {
                        !inputs.is_empty() && inputs.len() <= 100
                    }
                    _ => inputs.len() == 1,
                };
                *values += inputs.len();
                if !valid || *values > 100 {
                    return Err(
                        "filter operator has invalid values or exceeds the value budget".to_owned(),
                    );
                }
            }
        }
        Ok(())
    }
}

/// Maximum accepted offset-pagination page number.
pub const MAX_LIST_PAGE: u64 = 10_000;
/// Maximum accepted offset-pagination page size.
pub const MAX_LIST_PAGE_SIZE: u64 = 100;
/// Maximum Unicode scalar values accepted for a resource search term.
pub const MAX_SEARCH_CHARS: usize = 200;

/// Returns whether offset pagination arguments are within the generated API bounds.
#[must_use]
pub const fn list_page_is_valid(page: u64, page_size: u64) -> bool {
    page >= 1 && page <= MAX_LIST_PAGE && page_size >= 1 && page_size <= MAX_LIST_PAGE_SIZE
}

/// Returns whether a resource search term fits the generated API bound.
#[must_use]
pub fn search_term_is_valid(term: &str) -> bool {
    term.chars().count() <= MAX_SEARCH_CHARS
}

/// Builds a SQL `LIKE` pattern for substring search with `%`, `_`, and `\` escaped.
#[must_use]
pub fn like_contains_pattern(term: &str) -> String {
    let mut pattern = String::from("%");
    for character in term.chars() {
        if matches!(character, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::FilterExpression;
    use super::{
        MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE, MAX_SEARCH_CHARS, like_contains_pattern,
        list_page_is_valid, search_term_is_valid,
    };

    #[test]
    fn filter_trees_are_bounded_and_reject_ambiguous_inputs() {
        let valid = r#"{"all":[{"field":"status","op":"in","values":["todo","active"]},{"any":[{"field":"owner","op":"is_null"},{"field":"priority","op":"gte","values":["3"]}]}]}"#;
        assert!(FilterExpression::parse(valid).is_ok());
        for invalid in [
            r#"{"all":[]}"#,
            r#"{"field":"x","op":"in","values":[]}"#,
            r#"{"field":"x","op":"is_null","values":["null"]}"#,
            r#"{"field":"x","op":"eq","values":["1"],"sql":"TRUE"}"#,
            r#"{"field":"project.secret","op":"eq","values":["1"]}"#,
            r#"{"all":[],"any":[]}"#,
        ] {
            assert!(FilterExpression::parse(invalid).is_err(), "{invalid}");
        }
        let mut deep = r#"{"field":"x","op":"is_null"}"#.to_owned();
        for _ in 0..8 {
            deep = format!("{{\"all\":[{deep}]}}");
        }
        assert!(FilterExpression::parse(&deep).is_err());
        let wide = format!(
            "{{\"all\":[{}]}}",
            std::iter::repeat_n(r#"{"field":"x","op":"is_null"}"#, 64)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(FilterExpression::parse(&wide).is_err());
        assert!(FilterExpression::parse(&" ".repeat(16_385)).is_err());
    }

    #[test]
    fn list_pages_are_bounded() {
        assert!(list_page_is_valid(1, 25));
        assert!(list_page_is_valid(MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE));
        assert!(!list_page_is_valid(0, 25));
        assert!(!list_page_is_valid(MAX_LIST_PAGE + 1, 25));
        assert!(!list_page_is_valid(1, 0));
        assert!(!list_page_is_valid(1, MAX_LIST_PAGE_SIZE + 1));
    }

    #[test]
    fn like_contains_escapes_wildcards_and_backslashes() {
        assert_eq!(like_contains_pattern("ab"), "%ab%");
        assert_eq!(like_contains_pattern(""), "%%");
        assert_eq!(like_contains_pattern(r"a%b_c\d"), r"%a\%b\_c\\d%");
    }

    #[test]
    fn search_terms_are_bounded_by_unicode_characters() {
        assert!(search_term_is_valid(&"a".repeat(MAX_SEARCH_CHARS)));
        assert!(!search_term_is_valid(&"a".repeat(MAX_SEARCH_CHARS + 1)));
        assert!(search_term_is_valid(&"界".repeat(MAX_SEARCH_CHARS)));
        assert!(!search_term_is_valid(&"界".repeat(MAX_SEARCH_CHARS + 1)));
    }
}
