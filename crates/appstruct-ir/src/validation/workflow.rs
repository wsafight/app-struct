use super::{IrValidationError, push, validate_access_rule};
use crate::{EntityIr, FieldTypeIr};
use std::collections::BTreeSet;

pub(super) fn validate_workflow(
    entity: &EntityIr,
    path: &str,
    value_objects: &BTreeSet<&str>,
    errors: &mut Vec<IrValidationError>,
) {
    let Some(workflow) = &entity.workflow else {
        return;
    };
    let Some(field) = entity
        .fields
        .iter()
        .find(|field| field.id == workflow.field)
    else {
        push(
            errors,
            format!("{path}.workflow.field"),
            format!("references missing field `{}`", workflow.field),
        );
        return;
    };
    let FieldTypeIr::Enum { values } = &field.ty else {
        push(
            errors,
            format!("{path}.workflow.field"),
            "must reference an enum field",
        );
        return;
    };
    if field.nullable || field.primary_key || field.generated.is_some() || field.default.is_some() {
        push(
            errors,
            format!("{path}.workflow.field"),
            "must be a required, non-primary, non-generated field without a default",
        );
    }
    if !values.contains(&workflow.initial) {
        push(
            errors,
            format!("{path}.workflow.initial"),
            format!("`{}` is not a workflow field value", workflow.initial),
        );
    }
    if workflow.transitions.is_empty() {
        push(
            errors,
            format!("{path}.workflow.transitions"),
            "must not be empty",
        );
    }
    validate_transitions(entity, path, values, value_objects, errors);
}

fn validate_transitions(
    entity: &EntityIr,
    path: &str,
    values: &[String],
    value_objects: &BTreeSet<&str>,
    errors: &mut Vec<IrValidationError>,
) {
    let workflow = entity
        .workflow
        .as_ref()
        .expect("caller only validates workflow entities");
    let mut names = BTreeSet::new();
    let mut variants = BTreeSet::new();
    let mut edges = BTreeSet::new();
    for (index, transition) in workflow.transitions.iter().enumerate() {
        let transition_path = format!("{path}.workflow.transitions[{index}]");
        if !names.insert(transition.name.as_str()) {
            push(
                errors,
                format!("{transition_path}.name"),
                format!("duplicate transition `{}`", transition.name),
            );
        }
        let variant = rust_variant_name(&transition.name);
        if !variants.insert(variant.clone()) {
            push(
                errors,
                format!("{transition_path}.name"),
                format!("collides with another transition as Rust variant `{variant}`"),
            );
        }
        if transition.from.is_empty() {
            push(
                errors,
                format!("{transition_path}.from"),
                "must not be empty",
            );
        }
        for (state_index, state) in transition.from.iter().enumerate() {
            if !values.contains(state) {
                push(
                    errors,
                    format!("{transition_path}.from[{state_index}]"),
                    format!("`{state}` is not a workflow field value"),
                );
            }
            if !edges.insert((state.as_str(), transition.to.as_str())) {
                push(
                    errors,
                    transition_path.clone(),
                    format!(
                        "workflow edge `{state}` -> `{}` is declared more than once",
                        transition.to,
                    ),
                );
            }
        }
        if !values.contains(&transition.to) {
            push(
                errors,
                format!("{transition_path}.to"),
                format!("`{}` is not a workflow field value", transition.to),
            );
        }
        if transition.from.contains(&transition.to) {
            push(
                errors,
                transition_path.clone(),
                "cannot target one of its source states",
            );
        }
        if let Some(input) = &transition.input
            && !value_objects.contains(input.as_str())
        {
            push(
                errors,
                format!("{transition_path}.input"),
                format!("references missing value object `{input}`"),
            );
        }
        validate_access_rule(
            &transition.access,
            entity,
            &format!("{transition_path}.access"),
            errors,
        );
    }
}

fn rust_variant_name(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect()
}
