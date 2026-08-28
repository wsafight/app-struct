use super::super::surface::SurfaceEntity;
use appstruct_ir::{Diagnostic, EntityId, FieldIr, FieldTypeIr, SeedIr};
use std::collections::BTreeMap;

pub(super) fn build_seeds(
    entity: &SurfaceEntity,
    entity_id: &EntityId,
    fields: &[FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<SeedIr> {
    entity
        .seeds
        .iter()
        .filter_map(|seed| {
            if !crate::naming::is_sql_name(&seed.name.value) {
                diagnostics.push(Diagnostic::error(
                    "AS2060",
                    format!("invalid seed name `{}`", seed.name.value),
                    seed.name.span.clone(),
                ));
                return None;
            }
            let mut values = BTreeMap::new();
            let mut valid = true;
            for (field, value) in &seed.values {
                let Some(found) = fields.iter().find(|candidate| {
                    candidate.api_name == field.value || candidate.rust_name == field.value
                }) else {
                    diagnostics.push(Diagnostic::error(
                        "AS2061",
                        format!(
                            "seed `{}` references unknown field `{}`",
                            seed.name.value, field.value
                        ),
                        field.span.clone(),
                    ));
                    valid = false;
                    continue;
                };
                if !value_is_valid(&value.value, &found.ty) {
                    diagnostics.push(Diagnostic::error(
                        "AS2064",
                        format!(
                            "seed `{}` has an invalid value for field `{}`",
                            seed.name.value, field.value
                        ),
                        value.span.clone(),
                    ));
                    valid = false;
                    continue;
                }
                values.insert(found.api_name.clone(), value.value.clone());
            }
            if let Some(key) = fields.iter().find(|field| field.primary_key)
                && !values.contains_key(&key.api_name)
            {
                diagnostics.push(Diagnostic::error(
                    "AS2062",
                    format!(
                        "seed `{}` must provide primary-key field `{}`",
                        seed.name.value, key.api_name
                    ),
                    seed.span.clone(),
                ));
                valid = false;
            }
            for field in fields.iter().filter(|field| {
                !field.nullable && field.generated.is_none() && field.default.is_none()
            }) {
                if !values.contains_key(&field.api_name) {
                    diagnostics.push(Diagnostic::error(
                        "AS2063",
                        format!(
                            "seed `{}` is missing required field `{}`",
                            seed.name.value, field.api_name
                        ),
                        seed.span.clone(),
                    ));
                    valid = false;
                }
            }
            if !valid || values.is_empty() {
                return None;
            }
            Some(SeedIr {
                id: format!("app::{}::{}", entity.name.value, seed.name.value),
                entity: entity_id.clone(),
                values,
            })
        })
        .collect()
}

fn value_is_valid(value: &str, ty: &FieldTypeIr) -> bool {
    match ty {
        FieldTypeIr::Integer => value.parse::<i32>().is_ok(),
        FieldTypeIr::Bigint => value.parse::<i64>().is_ok(),
        FieldTypeIr::Decimal => value.parse::<f64>().is_ok_and(f64::is_finite),
        FieldTypeIr::Boolean => matches!(value, "true" | "false"),
        FieldTypeIr::Enum { values } => values.iter().any(|candidate| candidate == value),
        _ => true,
    }
}
