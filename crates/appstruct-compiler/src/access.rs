use crate::surface::{Located, SurfaceAccess, SurfaceAccessRule, SurfaceEntity, SurfaceField};
use appstruct_ir::{AccessRuleIr, AuthIr, CrudAccessIr, Diagnostic, FieldId};

pub(crate) fn build_access(
    entity: &SurfaceEntity,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CrudAccessIr> {
    let Some(access) = &entity.access else {
        diagnostics.push(
            Diagnostic::error(
                "AS3001",
                format!("entity `{}` has no access policy", entity.name.value),
                entity.span.clone(),
            )
            .with_help("declare list, read, create, update, and delete rules under `access`"),
        );
        return None;
    };
    let missing = missing_operations(access);
    if !missing.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "AS3002",
                format!("access policy is missing: {}", missing.join(", ")),
                access.span.clone(),
            )
            .with_help("every CRUD operation must be explicitly authorized"),
        );
        return None;
    }
    Some(CrudAccessIr {
        list: lower_rule(access.list.as_ref()?, Some(entity), auth, diagnostics)?,
        read: lower_rule(access.read.as_ref()?, Some(entity), auth, diagnostics)?,
        create: lower_rule(access.create.as_ref()?, Some(entity), auth, diagnostics)?,
        update: lower_rule(access.update.as_ref()?, Some(entity), auth, diagnostics)?,
        delete: lower_rule(access.delete.as_ref()?, Some(entity), auth, diagnostics)?,
    })
}

pub(crate) fn build_operation_access(
    rule: &Located<SurfaceAccessRule>,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<AccessRuleIr> {
    lower_rule(rule, None, auth, diagnostics)
}

pub(crate) fn build_field_access(
    field: &SurfaceField,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Option<AccessRuleIr>, Option<AccessRuleIr>) {
    let Some(access) = &field.access else {
        return (None, None);
    };
    let read = access
        .read
        .as_ref()
        .and_then(|rule| lower_rule(rule, None, auth, diagnostics));
    let write = access
        .write
        .as_ref()
        .and_then(|rule| lower_rule(rule, None, auth, diagnostics));
    (read, write)
}

fn lower_rule(
    rule: &Located<SurfaceAccessRule>,
    entity: Option<&SurfaceEntity>,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<AccessRuleIr> {
    let lowered = match &rule.value {
        SurfaceAccessRule::Public => AccessRuleIr::Public,
        SurfaceAccessRule::Authenticated => {
            require_auth(auth, rule, diagnostics)?;
            AccessRuleIr::Authenticated
        }
        SurfaceAccessRule::Role(role_name) => {
            require_auth(auth, rule, diagnostics)?;
            if !auth.roles.contains(role_name) {
                diagnostics.push(Diagnostic::error(
                    "AS3030",
                    format!("access rule references undeclared role `{role_name}`"),
                    rule.span.clone(),
                ));
                return None;
            }
            AccessRuleIr::Role {
                role: role_name.clone(),
            }
        }
        SurfaceAccessRule::Owner(field_name) => {
            require_auth(auth, rule, diagnostics)?;
            let Some(entity) = entity else {
                diagnostics.push(Diagnostic::error(
                    "AS3031",
                    "owner rules are only valid on entity operations",
                    rule.span.clone(),
                ));
                return None;
            };
            let Some(field) = entity
                .fields
                .iter()
                .find(|field| field.name.value == *field_name)
            else {
                diagnostics.push(Diagnostic::error(
                    "AS3032",
                    format!("owner rule references unknown field `{field_name}`"),
                    rule.span.clone(),
                ));
                return None;
            };
            let user_name = auth
                .user_entity
                .as_ref()
                .map(|id| id.0.trim_start_matches("app::"));
            if field.type_name.value != "relation"
                || field.target.as_ref().map(|target| target.value.as_str()) != user_name
            {
                diagnostics.push(Diagnostic::error(
                    "AS3033",
                    format!("owner field `{field_name}` must relate to the auth user entity"),
                    field.span.clone(),
                ));
                return None;
            }
            AccessRuleIr::Owner {
                field: FieldId(format!("app::{}.{field_name}", entity.name.value)),
            }
        }
        SurfaceAccessRule::Any(children) => AccessRuleIr::Any {
            rules: lower_children(children, entity, auth, diagnostics)?,
        },
        SurfaceAccessRule::All(children) => AccessRuleIr::All {
            rules: lower_children(children, entity, auth, diagnostics)?,
        },
    };
    Some(normalize(lowered))
}

fn lower_children(
    children: &[Located<SurfaceAccessRule>],
    entity: Option<&SurfaceEntity>,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<AccessRuleIr>> {
    let mut lowered = children
        .iter()
        .map(|child| lower_rule(child, entity, auth, diagnostics))
        .collect::<Option<Vec<_>>>()?;
    lowered.sort_by_key(access_key);
    lowered.dedup();
    Some(lowered)
}

fn require_auth(
    auth: &AuthIr,
    rule: &Located<SurfaceAccessRule>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<()> {
    if auth.enabled {
        return Some(());
    }
    diagnostics.push(Diagnostic::error(
        "AS3034",
        "authenticated access rule requires the Auth Module",
        rule.span.clone(),
    ));
    None
}

fn normalize(rule: AccessRuleIr) -> AccessRuleIr {
    match rule {
        AccessRuleIr::Any { rules } => AccessRuleIr::Any {
            rules: flatten(rules, true),
        },
        AccessRuleIr::All { rules } => AccessRuleIr::All {
            rules: flatten(rules, false),
        },
        other => other,
    }
}

fn flatten(rules: Vec<AccessRuleIr>, any: bool) -> Vec<AccessRuleIr> {
    let mut output = Vec::new();
    for rule in rules {
        match rule {
            AccessRuleIr::Any { rules } if any => output.extend(rules),
            AccessRuleIr::All { rules } if !any => output.extend(rules),
            other => output.push(other),
        }
    }
    output.sort_by_key(access_key);
    output.dedup();
    output
}

fn access_key(rule: &AccessRuleIr) -> String {
    match rule {
        AccessRuleIr::Public => "0:public".to_owned(),
        AccessRuleIr::Authenticated => "1:authenticated".to_owned(),
        AccessRuleIr::Role { role } => format!("2:role:{role}"),
        AccessRuleIr::Owner { field } => format!("3:owner:{field}"),
        AccessRuleIr::Any { rules } => format!(
            "4:any:{}",
            rules.iter().map(access_key).collect::<Vec<_>>().join("|")
        ),
        AccessRuleIr::All { rules } => format!(
            "5:all:{}",
            rules.iter().map(access_key).collect::<Vec<_>>().join("|")
        ),
    }
}

fn missing_operations(access: &SurfaceAccess) -> Vec<&'static str> {
    [
        ("list", access.list.is_none()),
        ("read", access.read.is_none()),
        ("create", access.create.is_none()),
        ("update", access.update.is_none()),
        ("delete", access.delete.is_none()),
    ]
    .into_iter()
    .filter_map(|(name, missing)| missing.then_some(name))
    .collect()
}
