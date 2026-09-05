use super::{IrValidationErrors, push};
use crate::{EntityIr, FieldTypeIr, GeneratedValueIr};
use std::collections::BTreeSet;

/// Validate ownership, bounded editing and workflow guards for aggregate collections.
///
/// # Errors
/// Returns every invalid aggregate declaration.
#[allow(clippy::too_many_lines)]
pub fn validate_aggregates(entities: &[EntityIr]) -> Result<(), IrValidationErrors> {
    let mut errors = Vec::new();
    let mut owned = BTreeSet::new();
    for parent in entities {
        let mut names = BTreeSet::new();
        for aggregate in &parent.views.aggregates {
            let path = format!("{}.aggregates.{}", parent.id, aggregate.name);
            let valid_name = !aggregate.name.is_empty()
                && aggregate.name.as_bytes()[0].is_ascii_lowercase()
                && aggregate
                    .name
                    .bytes()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'_');
            if !valid_name || !names.insert(&aggregate.name) {
                push(
                    &mut errors,
                    &path,
                    "aggregate names must be unique snake_case identifiers",
                );
            }
            if !(1..=100).contains(&aggregate.max_items) {
                push(&mut errors, &path, "max_items must be between 1 and 100");
            }
            let Some(child) = entities.iter().find(|entity| entity.id == aggregate.child) else {
                push(
                    &mut errors,
                    &path,
                    "aggregate references a missing child entity",
                );
                continue;
            };
            if !owned.insert(&child.id)
                || child.id == parent.id
                || !child.views.aggregates.is_empty()
            {
                push(
                    &mut errors,
                    &path,
                    "a child must have one owner; nested and cyclic aggregates are unsupported",
                );
            }
            if child.tenant_scoped != parent.tenant_scoped
                || child.views.soft_delete
                || child.workflow.is_some()
            {
                push(
                    &mut errors,
                    &path,
                    "parent and child must share tenant scope; child workflow and soft_delete are unsupported",
                );
            }
            for entity in [parent, child] {
                if !entity.concurrency.enabled
                    || !entity
                        .fields
                        .iter()
                        .any(|field| field.primary_key && matches!(field.ty, FieldTypeIr::Uuid))
                {
                    push(
                        &mut errors,
                        &path,
                        "aggregate entities require UUID primary keys and revisions",
                    );
                }
            }
            if !child
                .fields
                .iter()
                .any(|field| field.primary_key && field.generated == Some(GeneratedValueIr::UuidV7))
            {
                push(
                    &mut errors,
                    &path,
                    "aggregate children require server-generated uuid_v7 primary keys",
                );
            }
            if child
                .fields
                .iter()
                .any(|field| field.generated == Some(GeneratedValueIr::AutoIncrement))
            {
                push(
                    &mut errors,
                    &path,
                    "aggregate children do not support auto_increment fields",
                );
            }
            if !child.fields.iter().any(|field| {
                field.id == aggregate.relation
                    && !field.nullable
                    && field.default.is_none()
                    && field.generated.is_none()
                    && field.write_access.is_none()
                    && matches!(&field.ty, FieldTypeIr::Relation { target } if *target == parent.id)
            }) {
                push(
                    &mut errors,
                    &path,
                    "relation must be a required writable relation to the parent without a default or field write rule",
                );
            }
            let valid_states = parent.workflow_field().and_then(|field| match &field.ty {
                FieldTypeIr::Enum { values } => Some(values),
                _ => None,
            });
            let distinct = aggregate.states.iter().collect::<BTreeSet<_>>();
            if valid_states.is_some_and(|values| {
                aggregate.states.is_empty()
                    || aggregate.states.iter().any(|state| !values.contains(state))
            }) || (valid_states.is_none() && !aggregate.states.is_empty())
                || distinct.len() != aggregate.states.len()
            {
                push(
                    &mut errors,
                    &path,
                    "states must list distinct parent workflow states, or be empty when no workflow exists",
                );
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(IrValidationErrors(errors))
    }
}
