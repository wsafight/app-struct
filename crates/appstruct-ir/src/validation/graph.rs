use super::{IrValidationError, push};
use crate::{AppIr, EntityIr, ModuleOrigin, OperationTypeIr};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_services(
    ir: &AppIr,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    if ir.auth.enabled {
        let Some(user_id) = &ir.auth.user_entity else {
            push(
                errors,
                "auth.user_entity",
                "is required when auth is enabled",
            );
            return;
        };
        if let Some(user) = entities.get(user_id.0.as_str()) {
            if !user.fields.iter().any(|field| field.primary_key) {
                push(
                    errors,
                    "auth.user_entity",
                    "must reference an entity with a primary key",
                );
            }
            if !user.fields.iter().any(|field| field.api_name == "email") {
                push(
                    errors,
                    "auth.user_entity",
                    "must reference an entity with an `email` field",
                );
            }
        } else {
            push(
                errors,
                "auth.user_entity",
                format!("references missing entity `{user_id}`"),
            );
        }
        match &ir.auth.default_role {
            Some(role) if ir.auth.roles.contains(role) => {}
            Some(role) => push(
                errors,
                "auth.default_role",
                format!("`{role}` is not declared in `auth.roles`"),
            ),
            None => push(
                errors,
                "auth.default_role",
                "is required when auth is enabled",
            ),
        }
    }
    if (ir.tenant.enabled || ir.audit.enabled) && !ir.auth.enabled {
        push(
            errors,
            "auth.enabled",
            "must be true when tenant or audit is enabled",
        );
    }
}

pub(super) fn validate_operations(
    ir: &AppIr,
    entities: &BTreeMap<&str, &EntityIr>,
    value_objects: &BTreeSet<&str>,
    errors: &mut Vec<IrValidationError>,
) {
    for (index, command) in ir.commands.iter().enumerate() {
        validate_operation_type(
            &command.input,
            &format!("commands[{index}].input"),
            entities,
            value_objects,
            errors,
        );
        validate_operation_type(
            &command.output,
            &format!("commands[{index}].output"),
            entities,
            value_objects,
            errors,
        );
    }
    for (index, query) in ir.queries.iter().enumerate() {
        if let Some(input) = &query.input {
            validate_operation_type(
                input,
                &format!("queries[{index}].input"),
                entities,
                value_objects,
                errors,
            );
        }
        validate_operation_type(
            &query.output,
            &format!("queries[{index}].output"),
            entities,
            value_objects,
            errors,
        );
    }
}

fn validate_operation_type(
    ty: &OperationTypeIr,
    path: &str,
    entities: &BTreeMap<&str, &EntityIr>,
    value_objects: &BTreeSet<&str>,
    errors: &mut Vec<IrValidationError>,
) {
    match ty {
        OperationTypeIr::Entity { entity } if !entities.contains_key(entity.0.as_str()) => push(
            errors,
            path,
            format!("references missing entity `{entity}`"),
        ),
        OperationTypeIr::ValueObject { value_object }
            if !value_objects.contains(value_object.as_str()) =>
        {
            push(
                errors,
                path,
                format!("references missing value object `{value_object}`"),
            );
        }
        _ => {}
    }
}

pub(super) fn validate_modules(ir: &AppIr, errors: &mut Vec<IrValidationError>) {
    let mut names = BTreeSet::new();
    let mut orders = BTreeSet::new();
    let providers = ir
        .modules
        .iter()
        .flat_map(|module| module.provides.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    for (index, module) in ir.modules.iter().enumerate() {
        let path = format!("modules[{index}]");
        if !names.insert(module.name.as_str()) {
            push(
                errors,
                format!("{path}.name"),
                format!("duplicate module `{}`", module.name),
            );
        }
        if !orders.insert(module.startup_order) {
            push(
                errors,
                format!("{path}.startup_order"),
                format!("duplicate startup order `{}`", module.startup_order),
            );
        }
        for (required_index, capability) in module.requires.iter().enumerate() {
            if !providers.contains(capability.as_str()) {
                push(
                    errors,
                    format!("{path}.requires[{required_index}]"),
                    format!("capability `{capability}` has no provider"),
                );
            }
        }
        match module.origin {
            ModuleOrigin::Official
                if module.manifest_path.is_some()
                    || module.content_sha256.is_some()
                    || !module.artifacts.is_empty() =>
            {
                push(
                    errors,
                    path,
                    "official modules cannot carry local provenance or artifacts",
                );
            }
            ModuleOrigin::Local
                if module.manifest_path.is_none() || module.content_sha256.is_none() =>
            {
                push(
                    errors,
                    path,
                    "local modules require manifest path and content digest",
                );
            }
            _ => {}
        }
    }
}
