use super::Located;
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::{Diagnostic, SourceSpan};
use std::collections::BTreeMap;

pub(super) fn required<'mapping>(
    mapping: &'mapping BTreeMap<String, MappingEntry>,
    key: &str,
    parent_span: &SourceSpan,
) -> Result<&'mapping MappingEntry, Diagnostic> {
    mapping.get(key).ok_or_else(|| {
        Diagnostic::error(
            "AS1007",
            format!("missing required key `{key}`"),
            parent_span.clone(),
        )
    })
}

pub(super) fn ensure_known_keys(
    mapping: &BTreeMap<String, MappingEntry>,
    allowed: &[&str],
    context: &str,
) -> Result<(), Diagnostic> {
    let unknown = mapping
        .iter()
        .find(|(key, _)| !allowed.contains(&key.as_str()));
    let Some((key, entry)) = unknown else {
        return Ok(());
    };
    Err(Diagnostic::error(
        "AS1012",
        format!("unknown key `{key}` in {context}"),
        entry.key_span.clone(),
    )
    .with_help("remove the key or use a compiler version that supports this feature"))
}

pub(super) fn unknown_key_diagnostics(
    mapping: &BTreeMap<String, MappingEntry>,
    allowed: &[&str],
    context: &str,
) -> Vec<Diagnostic> {
    mapping
        .iter()
        .filter(|(key, _)| !allowed.contains(&key.as_str()))
        .map(|(key, entry)| {
            Diagnostic::error(
                "AS1012",
                format!("unknown key `{key}` in {context}"),
                entry.key_span.clone(),
            )
            .with_help("remove the key or use a compiler version that supports this feature")
        })
        .collect()
}

pub(super) fn expect_mapping<'node>(
    node: &'node Node,
    context: &str,
) -> Result<&'node BTreeMap<String, MappingEntry>, Diagnostic> {
    node.mapping().ok_or_else(|| {
        Diagnostic::error(
            "AS1007",
            format!("{context} must be a mapping"),
            node.span.clone(),
        )
    })
}

pub(super) fn expect_sequence<'node>(
    node: &'node Node,
    context: &str,
) -> Result<&'node [Node], Diagnostic> {
    node.sequence().ok_or_else(|| {
        Diagnostic::error(
            "AS1007",
            format!("{context} must be a sequence"),
            node.span.clone(),
        )
    })
}

pub(super) fn expect_string(node: &Node, context: &str) -> Result<Located<String>, Diagnostic> {
    let Some((value, plain)) = node.scalar() else {
        return Err(Diagnostic::error(
            "AS1007",
            format!("{context} must be a string"),
            node.span.clone(),
        ));
    };
    if plain && is_yaml_non_string(value) {
        return Err(Diagnostic::error(
            "AS1007",
            format!("{context} must be a string"),
            node.span.clone(),
        ));
    }
    Ok(Located {
        value: value.to_owned(),
        span: node.span.clone(),
    })
}

pub(super) fn expect_scalar_string(
    node: &Node,
    context: &str,
) -> Result<Located<String>, Diagnostic> {
    let Some((value, _)) = node.scalar() else {
        return Err(Diagnostic::error(
            "AS1007",
            format!("{context} must be a scalar"),
            node.span.clone(),
        ));
    };
    Ok(Located {
        value: value.to_owned(),
        span: node.span.clone(),
    })
}

pub(super) fn expect_bool(node: &Node, context: &str) -> Result<bool, Diagnostic> {
    let Some((value, true)) = node.scalar() else {
        return Err(Diagnostic::error(
            "AS1007",
            format!("{context} must be a boolean"),
            node.span.clone(),
        ));
    };
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Diagnostic::error(
            "AS1007",
            format!("{context} must be `true` or `false`"),
            node.span.clone(),
        )),
    }
}

pub(super) fn expect_u64(node: &Node, context: &str) -> Result<Located<u64>, Diagnostic> {
    let Some((value, true)) = node.scalar() else {
        return Err(Diagnostic::error(
            "AS1007",
            format!("{context} must be a non-negative integer"),
            node.span.clone(),
        ));
    };
    let parsed = value.parse().map_err(|_| {
        Diagnostic::error(
            "AS1007",
            format!("{context} must be a non-negative integer"),
            node.span.clone(),
        )
    })?;
    Ok(Located {
        value: parsed,
        span: node.span.clone(),
    })
}

pub(super) fn optional_string(
    mapping: &BTreeMap<String, MappingEntry>,
    key: &str,
    context: &str,
) -> Result<Option<Located<String>>, Diagnostic> {
    mapping
        .get(key)
        .map(|entry| expect_string(&entry.value, context))
        .transpose()
}

pub(super) fn optional_bool(
    mapping: &BTreeMap<String, MappingEntry>,
    key: &str,
) -> Result<bool, Diagnostic> {
    mapping
        .get(key)
        .map(|entry| expect_bool(&entry.value, &format!("field `{key}`")))
        .transpose()
        .map(Option::unwrap_or_default)
}

pub(super) fn optional_u64(
    mapping: &BTreeMap<String, MappingEntry>,
    key: &str,
) -> Result<Option<Located<u64>>, Diagnostic> {
    mapping
        .get(key)
        .map(|entry| expect_u64(&entry.value, &format!("field `{key}`")))
        .transpose()
}

fn is_yaml_non_string(value: &str) -> bool {
    matches!(
        value,
        "null" | "Null" | "NULL" | "~" | "true" | "false" | "True" | "False"
    ) || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
}
