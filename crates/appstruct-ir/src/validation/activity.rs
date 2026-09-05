use super::{IrValidationError, push};
use crate::{AppIr, EntityIr};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_activity(
    ir: &AppIr,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    let activity = &ir.activity;
    if !activity.enabled {
        if !activity.resources.is_empty() {
            push(
                errors,
                "activity.resources",
                "must be empty when activity is disabled",
            );
        }
        return;
    }
    if !ir.auth.enabled || !ir.audit.enabled {
        push(
            errors,
            "activity.enabled",
            "requires auth and audit modules to be enabled",
        );
    }
    if activity.attachments && !ir.file.enabled {
        push(
            errors,
            "activity.attachments",
            "requires the file module to be enabled",
        );
    }
    if !(1..=65_536).contains(&activity.max_comment_bytes) {
        push(
            errors,
            "activity.max_comment_bytes",
            "must be between 1 and 65536",
        );
    }
    if activity.resources.is_empty() {
        push(
            errors,
            "activity.resources",
            "must contain at least one entity",
        );
    }

    let mut roles = BTreeSet::new();
    for (index, role) in activity.admin_roles.iter().enumerate() {
        if !ir.auth.roles.contains(role) {
            push(
                errors,
                format!("activity.admin_roles[{index}]"),
                format!("unknown auth role `{role}`"),
            );
        }
        if !roles.insert(role) {
            push(
                errors,
                format!("activity.admin_roles[{index}]"),
                format!("duplicate role `{role}`"),
            );
        }
    }

    let mut entity_ids = BTreeSet::new();
    let mut resource_keys = BTreeSet::new();
    for (index, resource) in activity.resources.iter().enumerate() {
        let path = format!("activity.resources[{index}]");
        let Some(entity) = entities.get(resource.entity.0.as_str()) else {
            push(
                errors,
                format!("{path}.entity"),
                format!("references missing entity `{}`", resource.entity),
            );
            continue;
        };
        if entity.table_name != resource.resource {
            push(
                errors,
                format!("{path}.resource"),
                format!("must match entity table name `{}`", entity.table_name),
            );
        }
        if !entity_ids.insert(&resource.entity) {
            push(errors, format!("{path}.entity"), "duplicates an entity");
        }
        if !resource_keys.insert(resource.resource.as_str()) {
            push(
                errors,
                format!("{path}.resource"),
                "duplicates a resource key",
            );
        }
    }
}
