use crate::surface::{Located, SurfaceAccess, SurfaceAccessRule, SurfaceEntity};
use appstruct_ir::{AccessRuleIr, CrudAccessIr, Diagnostic};

pub(crate) fn build_access(
    entity: &SurfaceEntity,
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
        list: convert_rule(access.list.as_ref().expect("validated above")),
        read: convert_rule(access.read.as_ref().expect("validated above")),
        create: convert_rule(access.create.as_ref().expect("validated above")),
        update: convert_rule(access.update.as_ref().expect("validated above")),
        delete: convert_rule(access.delete.as_ref().expect("validated above")),
    })
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

pub(crate) fn convert_rule(rule: &Located<SurfaceAccessRule>) -> AccessRuleIr {
    match &rule.value {
        SurfaceAccessRule::Public => AccessRuleIr::Public,
        SurfaceAccessRule::Role(role_name) => AccessRuleIr::Role {
            role: role_name.clone(),
        },
    }
}
