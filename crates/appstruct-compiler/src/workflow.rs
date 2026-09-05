use crate::access::build_entity_operation_access;
use crate::naming::is_rust_field_name;
use crate::surface::{SurfaceEntity, SurfaceWorkflow};
use appstruct_ir::{AuthIr, Diagnostic, FieldIr, FieldTypeIr, WorkflowIr, WorkflowTransitionIr};
use std::collections::BTreeSet;

#[allow(clippy::too_many_lines)]
pub(crate) fn lower_workflow(
    entity: &SurfaceEntity,
    fields: &[FieldIr],
    known_values: &BTreeSet<String>,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<WorkflowIr> {
    let workflow = entity.workflow.as_ref()?;
    let Some(surface_field) = entity
        .fields
        .iter()
        .find(|field| field.name.value == workflow.field.value)
    else {
        diagnostics.push(Diagnostic::error(
            "AS3080",
            format!(
                "workflow references unknown field `{}`",
                workflow.field.value
            ),
            workflow.field.span.clone(),
        ));
        return None;
    };
    let field_id = format!("app::{}.{}", entity.name.value, workflow.field.value);
    let field = fields.iter().find(|field| field.id.0 == field_id)?;
    let FieldTypeIr::Enum { values } = &field.ty else {
        diagnostics.push(Diagnostic::error(
            "AS3081",
            "workflow field must be an enum",
            workflow.field.span.clone(),
        ));
        return None;
    };
    if !surface_field.flags.required() {
        diagnostics.push(Diagnostic::error(
            "AS3082",
            "workflow field must declare `required: true`",
            surface_field.span.clone(),
        ));
    }
    if surface_field.generated.is_some() || surface_field.default.is_some() {
        diagnostics.push(Diagnostic::error(
            "AS3083",
            "workflow field cannot declare `generated` or `default`; `initial` owns creation",
            surface_field.span.clone(),
        ));
    }
    if surface_field
        .access
        .as_ref()
        .is_some_and(|access| access.write.is_some())
    {
        diagnostics.push(Diagnostic::error(
            "AS3084",
            "workflow field cannot declare field-level write access",
            surface_field.span.clone(),
        ));
    }
    validate_state(
        &workflow.initial.value,
        values,
        "initial",
        &workflow.initial.span,
        diagnostics,
    );
    let mut rust_variants = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let transitions = workflow
        .transitions
        .iter()
        .filter_map(|transition| {
            if !is_rust_field_name(&transition.name.value) {
                diagnostics.push(Diagnostic::error(
                    "AS3085",
                    format!(
                        "invalid workflow transition name `{}`",
                        transition.name.value
                    ),
                    transition.name.span.clone(),
                ));
            }
            let variant = rust_variant_name(&transition.name.value);
            if !rust_variants.insert(variant.clone()) {
                diagnostics.push(Diagnostic::error(
                    "AS3091",
                    format!(
                        "workflow transition `{}` collides with another transition as Rust variant `{variant}`",
                        transition.name.value,
                    ),
                    transition.name.span.clone(),
                ));
            }
            let mut from = transition
                .from
                .iter()
                .map(|state| {
                    validate_state(&state.value, values, "source", &state.span, diagnostics);
                    state.value.clone()
                })
                .collect::<Vec<_>>();
            from.sort();
            let before_dedup = from.len();
            from.dedup();
            if from.len() != before_dedup {
                diagnostics.push(Diagnostic::error(
                    "AS3086",
                    format!(
                        "workflow transition `{}` repeats a source state",
                        transition.name.value
                    ),
                    transition.span.clone(),
                ));
            }
            validate_state(
                &transition.to.value,
                values,
                "target",
                &transition.to.span,
                diagnostics,
            );
            if from.iter().any(|state| state == &transition.to.value) {
                diagnostics.push(Diagnostic::error(
                    "AS3087",
                    format!(
                        "workflow transition `{}` cannot target its source state",
                        transition.name.value
                    ),
                    transition.to.span.clone(),
                ));
            }
            for source in &from {
                if !edges.insert((source.clone(), transition.to.value.clone())) {
                    diagnostics.push(Diagnostic::error(
                        "AS3092",
                        format!(
                            "workflow edge `{source}` -> `{}` is declared more than once",
                            transition.to.value,
                        ),
                        transition.span.clone(),
                    ));
                }
            }
            let input = transition.input.as_ref().and_then(|input| {
                if known_values.contains(&input.value) {
                    Some(format!("app::{}", input.value))
                } else {
                    diagnostics.push(Diagnostic::error(
                        "AS3088",
                        format!(
                            "workflow transition input references unknown value object `{}`",
                            input.value
                        ),
                        input.span.clone(),
                    ));
                    None
                }
            });
            Some(WorkflowTransitionIr {
                name: transition.name.value.clone(),
                from,
                to: transition.to.value.clone(),
                input,
                access: build_entity_operation_access(
                    &transition.access,
                    entity,
                    auth,
                    diagnostics,
                )?,
            })
        })
        .collect::<Vec<_>>();
    validate_reachability(workflow, values, diagnostics);
    Some(WorkflowIr {
        field: field.id.clone(),
        initial: workflow.initial.value.clone(),
        transitions,
    })
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

fn validate_state(
    state: &str,
    values: &[String],
    position: &str,
    span: &appstruct_ir::SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !values.iter().any(|value| value == state) {
        diagnostics.push(Diagnostic::error(
            "AS3089",
            format!("workflow {position} state `{state}` is not a field enum value"),
            span.clone(),
        ));
    }
}

fn validate_reachability(
    workflow: &SurfaceWorkflow,
    values: &[String],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut reachable = BTreeSet::from([workflow.initial.value.clone()]);
    loop {
        let before = reachable.len();
        for transition in &workflow.transitions {
            if transition
                .from
                .iter()
                .any(|state| reachable.contains(&state.value))
            {
                reachable.insert(transition.to.value.clone());
            }
        }
        if reachable.len() == before {
            break;
        }
    }
    let unreachable = values
        .iter()
        .filter(|state| !reachable.contains(*state))
        .cloned()
        .collect::<Vec<_>>();
    if !unreachable.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "AS3090",
                format!(
                    "workflow has states unreachable from `{}`: {}",
                    workflow.initial.value,
                    unreachable.join(", ")
                ),
                workflow.span.clone(),
            )
            .with_help("add transitions from a reachable state or remove unused enum values"),
        );
    }
}
