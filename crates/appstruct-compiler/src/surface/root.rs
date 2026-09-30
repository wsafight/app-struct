use super::Located;
use super::value::{
    ensure_known_keys, expect_mapping, expect_sequence, expect_string, expect_u64, required,
};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::Diagnostic;

pub(super) fn decode_version(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
    root: &Node,
) -> Result<Located<u64>, Diagnostic> {
    let node = required(mapping, "version", &root.span)?;
    expect_u64(&node.value, "`version`")
}

pub(super) fn decode_app_name(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
    root: &Node,
) -> Result<Located<String>, Diagnostic> {
    let node = required(mapping, "app", &root.span)?;
    let app = expect_mapping(&node.value, "`app`")?;
    ensure_known_keys(app, &["name"], "`app`")?;
    let name = required(app, "name", &node.value.span)?;
    expect_string(&name.value, "`app.name`")
}

pub(super) fn decode_string_list(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
    key: &str,
    required_key: bool,
    root: &Node,
) -> Result<Vec<Located<String>>, Diagnostic> {
    let entry = if required_key {
        Some(required(mapping, key, &root.span)?)
    } else {
        mapping.get(key)
    };
    let Some(entry) = entry else {
        return Ok(Vec::new());
    };
    expect_sequence(&entry.value, &format!("`{key}`"))?
        .iter()
        .map(|node| expect_string(node, &format!("{key} path")))
        .collect()
}
