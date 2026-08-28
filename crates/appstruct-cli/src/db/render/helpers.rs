use super::SpecType;
use std::collections::BTreeSet;

pub(super) fn render_capabilities(output: &mut String, kind: &SpecType, relation: bool) {
    if relation {
        output.push_str("        filterable: true\n        sortable: true\n");
    } else {
        match kind {
            SpecType::String => {
                output.push_str("        searchable: true\n        sortable: true\n");
            }
            SpecType::Text => output.push_str("        searchable: true\n"),
            SpecType::Json | SpecType::Unsupported(_) => {}
            _ => output.push_str("        filterable: true\n        sortable: true\n"),
        }
    }
}

pub(super) fn scalar_default(value: &str, kind: &SpecType) -> Option<String> {
    let stripped = value.split("::").next()?.trim().trim_matches(['(', ')']);
    match kind {
        SpecType::String | SpecType::Text | SpecType::Enum(_) => stripped
            .strip_prefix('\'')?
            .strip_suffix('\'')
            .map(|text| text.replace("''", "'")),
        SpecType::Integer | SpecType::Bigint | SpecType::Decimal => {
            stripped.parse::<f64>().ok().map(|_| stripped.to_owned())
        }
        SpecType::Boolean if matches!(stripped, "true" | "false") => Some(stripped.to_owned()),
        _ => None,
    }
}

pub(super) fn is_sequence_default(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim_start().starts_with("nextval("))
}

pub(super) fn is_now_default(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "now()" | "current_timestamp"
    )
}

pub(super) fn singularize(value: &str) -> String {
    if let Some(stem) = value.strip_suffix("ies") {
        format!("{stem}y")
    } else if let Some(stem) = value.strip_suffix("ses") {
        format!("{stem}s")
    } else if value.ends_with('s') && !value.ends_with("ss") && !value.ends_with("us") {
        value[..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

pub(super) fn type_name(value: &str) -> String {
    let mut output = words(value)
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    chars.as_str().to_ascii_lowercase()
                )
            })
        })
        .collect::<String>();
    if output
        .chars()
        .next()
        .is_none_or(|character| !character.is_ascii_uppercase())
    {
        output.insert_str(0, "Imported");
    }
    output
}

pub(super) fn field_name(value: &str) -> String {
    let mut output = words(value)
        .collect::<Vec<_>>()
        .join("_")
        .to_ascii_lowercase();
    if output
        .chars()
        .next()
        .is_none_or(|character| !character.is_ascii_lowercase())
    {
        output.insert_str(0, "field_");
    }
    if is_rust_keyword(&output) {
        output.push_str("_value");
    }
    output
}

fn words(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
}

pub(super) fn unique_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let mut candidate = base.to_owned();
    let mut suffix = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}{suffix}");
        suffix += 1;
    }
    candidate
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "async"
            | "await"
            | "const"
            | "crate"
            | "dyn"
            | "enum"
            | "extern"
            | "fn"
            | "for"
            | "impl"
            | "in"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

pub(super) fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).expect("strings always serialize")
}
