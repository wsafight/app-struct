use super::access;
use super::model::{Located, SurfaceWorkflow, SurfaceWorkflowTransition};
use super::value::{
    ensure_known_keys, expect_mapping, expect_sequence, expect_string, optional_string, required,
};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::Diagnostic;

pub(super) fn decode_workflow(node: &Node) -> Result<SurfaceWorkflow, Diagnostic> {
    let mapping = expect_mapping(node, "entity `workflow`")?;
    ensure_known_keys(
        mapping,
        &["field", "initial", "transitions"],
        "entity `workflow`",
    )?;
    let field = required(mapping, "field", &node.span)?;
    let initial = required(mapping, "initial", &node.span)?;
    let transitions = required(mapping, "transitions", &node.span)?;
    let transitions = expect_mapping(&transitions.value, "workflow `transitions`")?;
    if transitions.is_empty() {
        return Err(Diagnostic::error(
            "AS1007",
            "workflow `transitions` must not be empty",
            node.span.clone(),
        ));
    }
    let transitions = transitions
        .iter()
        .map(|(name, entry)| decode_transition(name, entry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SurfaceWorkflow {
        field: expect_string(&field.value, "workflow `field`")?,
        initial: expect_string(&initial.value, "workflow `initial`")?,
        transitions,
        span: node.span.clone(),
    })
}

fn decode_transition(
    name: &str,
    entry: &MappingEntry,
) -> Result<SurfaceWorkflowTransition, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "workflow transition")?;
    ensure_known_keys(
        mapping,
        &["from", "to", "input", "access"],
        "workflow transition",
    )?;
    let from = required(mapping, "from", &entry.value.span)?;
    let from = expect_sequence(&from.value, "workflow transition `from`")?
        .iter()
        .map(|value| expect_string(value, "workflow transition source state"))
        .collect::<Result<Vec<_>, _>>()?;
    if from.is_empty() {
        return Err(Diagnostic::error(
            "AS1007",
            "workflow transition `from` must not be empty",
            entry.value.span.clone(),
        ));
    }
    let to = required(mapping, "to", &entry.value.span)?;
    let access = required(mapping, "access", &entry.value.span)?;
    Ok(SurfaceWorkflowTransition {
        name: Located {
            value: name.to_owned(),
            span: entry.key_span.clone(),
        },
        from,
        to: expect_string(&to.value, "workflow transition `to`")?,
        input: optional_string(mapping, "input", "workflow transition `input`")?,
        access: access::decode_rule(&access.value)?,
        span: entry.value.span.clone(),
    })
}
