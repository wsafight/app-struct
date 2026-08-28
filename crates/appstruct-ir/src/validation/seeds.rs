use super::push;
use crate::{AppIr, EntityIr};
use std::collections::BTreeSet;

pub(super) fn validate_seeds(
    ir: &AppIr,
    entities: &std::collections::BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<super::IrValidationError>,
) {
    let mut ids = BTreeSet::new();
    for (index, seed) in ir.seeds.iter().enumerate() {
        let path = format!("seeds[{index}]");
        if !ids.insert(seed.id.as_str()) {
            push(
                errors,
                format!("{path}.id"),
                format!("duplicate seed id `{}`", seed.id),
            );
        }
        let Some(entity) = entities.get(seed.entity.0.as_str()) else {
            push(
                errors,
                format!("{path}.entity"),
                format!("references missing entity `{}`", seed.entity),
            );
            continue;
        };
        let fields = entity
            .fields
            .iter()
            .map(|field| field.api_name.as_str())
            .collect::<BTreeSet<_>>();
        for field in seed.values.keys() {
            if !fields.contains(field.as_str()) {
                push(
                    errors,
                    format!("{path}.values.{field}"),
                    format!("references missing field `{field}`"),
                );
            }
        }
    }
}
