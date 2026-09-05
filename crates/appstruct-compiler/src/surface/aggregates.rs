use super::value::{
    ensure_known_keys, expect_mapping, expect_sequence, expect_string, optional_u64, required,
};
use crate::yaml::Node;
use appstruct_ir::{AggregateIr, Diagnostic, EntityId, FieldId};

pub(super) fn decode(node: &Node) -> Result<Vec<AggregateIr>, Diagnostic> {
    expect_mapping(node, "aggregates")?
        .iter()
        .map(|(name, entry)| {
            let mapping = expect_mapping(&entry.value, "aggregate")?;
            ensure_known_keys(
                mapping,
                &["entity", "relation", "states", "max_items"],
                "aggregate",
            )?;
            let child = expect_string(
                &required(mapping, "entity", &entry.value.span)?.value,
                "aggregate entity",
            )?;
            let relation = expect_string(
                &required(mapping, "relation", &entry.value.span)?.value,
                "aggregate relation",
            )?;
            let states = mapping
                .get("states")
                .map(|states| {
                    expect_sequence(&states.value, "aggregate states")?
                        .iter()
                        .map(|value| {
                            expect_string(value, "aggregate state").map(|value| value.value)
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            let max_items = optional_u64(mapping, "max_items")?.map_or(100, |value| value.value);
            Ok(AggregateIr {
                name: name.clone(),
                child: EntityId(format!("app::{}", child.value)),
                relation: FieldId(format!("app::{}.{}", child.value, relation.value)),
                states,
                max_items: u32::try_from(max_items).unwrap_or(u32::MAX),
            })
        })
        .collect()
}
