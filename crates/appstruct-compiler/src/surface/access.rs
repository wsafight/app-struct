use super::value::{ensure_known_keys, expect_bool, expect_mapping, expect_string};
use super::{Located, SurfaceAccess, SurfaceAccessRule};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::Diagnostic;
use std::collections::BTreeMap;

pub(super) fn decode_crud_access(node: &Node) -> Result<SurfaceAccess, Diagnostic> {
    let mapping = expect_mapping(node, "entity `access`")?;
    ensure_known_keys(
        mapping,
        &["list", "read", "create", "update", "delete"],
        "entity `access`",
    )?;
    Ok(SurfaceAccess {
        list: optional_rule(mapping, "list")?,
        read: optional_rule(mapping, "read")?,
        create: optional_rule(mapping, "create")?,
        update: optional_rule(mapping, "update")?,
        delete: optional_rule(mapping, "delete")?,
        span: node.span.clone(),
    })
}

pub(super) fn decode_rule(node: &Node) -> Result<Located<SurfaceAccessRule>, Diagnostic> {
    let rule = expect_mapping(node, "access rule")?;
    ensure_known_keys(rule, &["role", "public"], "access rule")?;
    let value = if let Some(role) = rule.get("role") {
        SurfaceAccessRule::Role(expect_string(&role.value, "access `role`")?.value)
    } else if let Some(public) = rule.get("public") {
        if !expect_bool(&public.value, "access `public`")? {
            return Err(Diagnostic::error(
                "AS1007",
                "`public` must be true when present",
                public.value.span.clone(),
            ));
        }
        SurfaceAccessRule::Public
    } else {
        return Err(Diagnostic::error(
            "AS1007",
            "access rule requires `role` or `public: true`",
            node.span.clone(),
        ));
    };
    Ok(Located {
        value,
        span: node.span.clone(),
    })
}

fn optional_rule(
    mapping: &BTreeMap<String, MappingEntry>,
    operation: &str,
) -> Result<Option<Located<SurfaceAccessRule>>, Diagnostic> {
    mapping
        .get(operation)
        .map(|entry| decode_rule(&entry.value))
        .transpose()
}
