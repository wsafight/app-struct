use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string,
};
use super::{Located, SurfaceAccess, SurfaceAccessRule, SurfaceFieldAccess};
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

pub(super) fn decode_field_access(node: &Node) -> Result<SurfaceFieldAccess, Diagnostic> {
    let mapping = expect_mapping(node, "field `access`")?;
    ensure_known_keys(mapping, &["read", "write"], "field `access`")?;
    Ok(SurfaceFieldAccess {
        read: optional_rule(mapping, "read")?,
        write: optional_rule(mapping, "write")?,
    })
}

pub(super) fn decode_rule(node: &Node) -> Result<Located<SurfaceAccessRule>, Diagnostic> {
    let rule = expect_mapping(node, "access rule")?;
    ensure_known_keys(
        rule,
        &["role", "public", "authenticated", "owner", "any", "all"],
        "access rule",
    )?;
    if rule.len() != 1 {
        return Err(Diagnostic::error(
            "AS1007",
            "access rule must contain exactly one expression",
            node.span.clone(),
        ));
    }
    let value = if let Some(role) = rule.get("role") {
        SurfaceAccessRule::Role(expect_string(&role.value, "access `role`")?.value)
    } else if let Some(owner) = rule.get("owner") {
        SurfaceAccessRule::Owner(expect_string(&owner.value, "access `owner`")?.value)
    } else if let Some(public) = rule.get("public") {
        if !expect_bool(&public.value, "access `public`")? {
            return Err(Diagnostic::error(
                "AS1007",
                "`public` must be true when present",
                public.value.span.clone(),
            ));
        }
        SurfaceAccessRule::Public
    } else if let Some(authenticated) = rule.get("authenticated") {
        if !expect_bool(&authenticated.value, "access `authenticated`")? {
            return Err(Diagnostic::error(
                "AS1007",
                "`authenticated` must be true when present",
                authenticated.value.span.clone(),
            ));
        }
        SurfaceAccessRule::Authenticated
    } else if let Some(any) = rule.get("any") {
        SurfaceAccessRule::Any(decode_children(&any.value, "access `any`")?)
    } else if let Some(all) = rule.get("all") {
        SurfaceAccessRule::All(decode_children(&all.value, "access `all`")?)
    } else {
        return Err(Diagnostic::error(
            "AS1007",
            "access rule requires a supported expression",
            node.span.clone(),
        ));
    };
    Ok(Located {
        value,
        span: node.span.clone(),
    })
}

fn decode_children(
    node: &Node,
    context: &str,
) -> Result<Vec<Located<SurfaceAccessRule>>, Diagnostic> {
    let children = expect_sequence(node, context)?;
    if children.is_empty() {
        return Err(Diagnostic::error(
            "AS1007",
            format!("{context} must contain at least one rule"),
            node.span.clone(),
        ));
    }
    children.iter().map(decode_rule).collect()
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
