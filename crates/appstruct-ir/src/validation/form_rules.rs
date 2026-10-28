use super::{IrValidationErrors, push};
use crate::{AccessRuleIr, ConditionValueIr, EntityIr, FieldIr, FieldTypeIr};

/// Check the portable, same-record validation contract without executing application code.
///
/// # Errors
/// Returns errors for invalid references, incompatible types or restricted field dependencies.
pub fn validate_form_rules(entities: &[EntityIr]) -> Result<(), IrValidationErrors> {
    let mut errors = Vec::new();
    for (entity_index, entity) in entities.iter().enumerate() {
        for (index, field) in entity.fields.iter().enumerate() {
            let path = format!("entities[{entity_index}].fields[{index}].validation");
            let rules = &field.validation;
            if rules.required_when.is_none() && rules.greater_than.is_none() {
                continue;
            }
            if field.primary_key
                || field.generated.is_some()
                || entity.is_workflow_field(field)
                || !unrestricted(field)
            {
                push(
                    &mut errors,
                    &path,
                    "validation target must be an ordinary field with unrestricted field access",
                );
            }
            if let Some(rule) = &rules.required_when {
                if !field.nullable
                    || matches!(field.ty, FieldTypeIr::Json | FieldTypeIr::Relation { .. })
                {
                    push(
                        &mut errors,
                        &path,
                        "required_when requires a nullable scalar target",
                    );
                }
                match entity
                    .fields
                    .iter()
                    .find(|other| other.id == rule.field && other.id != field.id)
                {
                    Some(other)
                        if unrestricted(other)
                            && match (&other.ty, &rule.equals) {
                                (FieldTypeIr::Boolean, ConditionValueIr::Boolean(_))
                                | (
                                    FieldTypeIr::String | FieldTypeIr::Text,
                                    ConditionValueIr::String(_),
                                ) => true,
                                (FieldTypeIr::Enum { values }, ConditionValueIr::String(value)) => {
                                    values.contains(value)
                                }
                                _ => false,
                            } => {}
                    _ => push(
                        &mut errors,
                        format!("{path}.required_when"),
                        "must reference a different unrestricted sibling boolean/text/enum field with a matching equals value",
                    ),
                }
            }
            if let Some(id) = &rules.greater_than {
                if !matches!(
                    field.ty,
                    FieldTypeIr::Integer
                        | FieldTypeIr::Bigint
                        | FieldTypeIr::Decimal
                        | FieldTypeIr::Date
                        | FieldTypeIr::Datetime
                ) {
                    push(
                        &mut errors,
                        &path,
                        "greater_than requires a numeric, date or datetime target",
                    );
                }
                if !entity.fields.iter().any(|other| {
                    other.id == *id
                        && other.id != field.id
                        && other.ty == field.ty
                        && unrestricted(other)
                }) {
                    push(
                        &mut errors,
                        format!("{path}.greater_than"),
                        "must reference a different unrestricted sibling field of the same type",
                    );
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(IrValidationErrors(errors))
    }
}

fn unrestricted(field: &FieldIr) -> bool {
    [field.read_access.as_ref(), field.write_access.as_ref()]
        .iter()
        .all(|rule| rule.is_none_or(|rule| *rule == AccessRuleIr::Public))
}
