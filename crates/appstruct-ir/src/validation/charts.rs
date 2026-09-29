use super::{IrValidationErrors, push};
use crate::{ChartDimensionIr, ChartKindIr, ChartMeasureIr, EntityIr, FieldIr, FieldTypeIr};
use std::collections::BTreeSet;

/// Validate bounded, typed chart declarations.
///
/// # Errors
/// Returns every invalid chart declaration.
pub fn validate_charts(entities: &[EntityIr]) -> Result<(), IrValidationErrors> {
    let mut errors = Vec::new();
    for entity in entities {
        if entity.views.charts.len() > 20 {
            push(
                &mut errors,
                format!("{}.charts", entity.id),
                "an entity can declare at most 20 charts",
            );
        }
        let mut names = BTreeSet::new();
        for chart in &entity.views.charts {
            let path = format!("{}.charts.{}", entity.id, chart.name);
            if !valid_name(&chart.name) || !names.insert(&chart.name) {
                push(
                    &mut errors,
                    &path,
                    "chart names must be unique snake_case identifiers",
                );
            }
            if chart.label.trim().is_empty() || chart.label.chars().count() > 120 {
                push(
                    &mut errors,
                    &path,
                    "chart labels must contain between 1 and 120 characters",
                );
            }
            if !(1..=100).contains(&chart.limit) {
                push(&mut errors, &path, "chart limit must be between 1 and 100");
            }
            match (&chart.kind, &chart.dimension) {
                (ChartKindIr::Kpi, None) => {}
                (ChartKindIr::Kpi, Some(_)) => push(
                    &mut errors,
                    &path,
                    "KPI charts must not declare a dimension",
                ),
                (_, None) => push(
                    &mut errors,
                    &path,
                    "bar and donut charts require a dimension",
                ),
                (_, Some(dimension)) => {
                    validate_dimension(entities, entity, dimension, &path, &mut errors);
                }
            }
            validate_measure(entity, &chart.measure, &path, &mut errors);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(IrValidationErrors(errors))
    }
}

fn validate_dimension(
    entities: &[EntityIr],
    entity: &EntityIr,
    dimension: &ChartDimensionIr,
    path: &str,
    errors: &mut Vec<super::IrValidationError>,
) {
    match dimension {
        ChartDimensionIr::Field { field } => {
            let Some(field) = entity
                .fields
                .iter()
                .find(|candidate| candidate.id == *field)
            else {
                push(errors, path, "chart dimension references a missing field");
                return;
            };
            if !field.capabilities.filterable || !groupable(&field.ty) {
                push(
                    errors,
                    path,
                    "chart dimensions must reference filterable scalar fields other than JSON",
                );
            }
        }
        ChartDimensionIr::RelationField { relation, field } => {
            let Some(relation) = entity
                .fields
                .iter()
                .find(|candidate| candidate.id == *relation)
            else {
                push(
                    errors,
                    path,
                    "chart dimension references a missing relation",
                );
                return;
            };
            let FieldTypeIr::Relation { target } = &relation.ty else {
                push(
                    errors,
                    path,
                    "chart relation dimension must start with a relation field",
                );
                return;
            };
            let Some(target) = entities.iter().find(|candidate| candidate.id == *target) else {
                push(errors, path, "chart dimension relation target is missing");
                return;
            };
            if target.id == entity.id {
                push(
                    errors,
                    path,
                    "self-relation chart dimensions are not supported",
                );
                return;
            }
            let Some(field) = target
                .fields
                .iter()
                .find(|candidate| candidate.id == *field)
            else {
                push(
                    errors,
                    path,
                    "chart dimension references a missing target field",
                );
                return;
            };
            if !relation.capabilities.filterable
                || !field.capabilities.filterable
                || !groupable(&field.ty)
            {
                push(
                    errors,
                    path,
                    "relation chart dimensions require filterable relation and target scalar fields",
                );
            }
        }
    }
}

fn validate_measure(
    entity: &EntityIr,
    measure: &ChartMeasureIr,
    path: &str,
    errors: &mut Vec<super::IrValidationError>,
) {
    let (field_id, operation) = match measure {
        ChartMeasureIr::Count => return,
        ChartMeasureIr::Sum { field } => (field, "sum"),
        ChartMeasureIr::Avg { field } => (field, "avg"),
        ChartMeasureIr::Min { field } => (field, "min"),
        ChartMeasureIr::Max { field } => (field, "max"),
    };
    let Some(field) = entity
        .fields
        .iter()
        .find(|candidate| candidate.id == *field_id)
    else {
        push(errors, path, "chart measure references a missing field");
        return;
    };
    let supported = match operation {
        "sum" | "avg" => supports_sum_avg(field),
        "min" | "max" => supports_min_max(field),
        _ => false,
    };
    if !field.capabilities.filterable || !supported {
        push(
            errors,
            path,
            format!("chart measure `{operation}` is not supported for this field"),
        );
    }
}

fn supports_sum_avg(field: &FieldIr) -> bool {
    matches!(
        field.ty,
        FieldTypeIr::Integer | FieldTypeIr::Bigint | FieldTypeIr::Decimal
    )
}

fn supports_min_max(field: &FieldIr) -> bool {
    matches!(
        field.ty,
        FieldTypeIr::Integer
            | FieldTypeIr::Bigint
            | FieldTypeIr::Decimal
            | FieldTypeIr::String
            | FieldTypeIr::Enum { .. }
            | FieldTypeIr::Date
            | FieldTypeIr::Datetime
    )
}

fn groupable(field_type: &FieldTypeIr) -> bool {
    !matches!(field_type, FieldTypeIr::Json | FieldTypeIr::Relation { .. })
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.bytes().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == b'_'
        })
}
