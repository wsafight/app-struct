use super::{IrValidationError, push};
use crate::{AppIr, EntityIr, FieldIr, FieldSemanticIr, FieldTypeIr};
use std::collections::BTreeSet;

pub(super) fn validate_field_semantics(
    ir: &AppIr,
    entity: &EntityIr,
    path: &str,
    errors: &mut Vec<IrValidationError>,
) {
    let mut companions = BTreeSet::new();
    validate_display_field(entity, path, errors);
    for (field_index, field) in entity.fields.iter().enumerate() {
        let field_path = format!("{path}.fields[{field_index}].ui_semantic");
        match &field.ui_semantic {
            None => {}
            Some(FieldSemanticIr::File) => {
                validate_file_semantic(ir, field, &field_path, errors);
            }
            Some(FieldSemanticIr::Money {
                currency_field,
                fraction_digits,
            }) => {
                validate_money_semantic(
                    entity,
                    field,
                    currency_field,
                    *fraction_digits,
                    &field_path,
                    &mut companions,
                    errors,
                );
            }
        }
    }
}

fn validate_money_semantic<'ir>(
    entity: &'ir EntityIr,
    field: &FieldIr,
    currency_field: &'ir crate::FieldId,
    fraction_digits: u8,
    field_path: &str,
    companions: &mut BTreeSet<&'ir crate::FieldId>,
    errors: &mut Vec<IrValidationError>,
) {
    if field.ui_component.is_some() {
        push(
            errors,
            field_path,
            "cannot be combined with a custom UI component",
        );
    }
    if !matches!(field.ty, FieldTypeIr::Decimal) || field.generated.is_some() {
        push(
            errors,
            field_path,
            "money requires a non-generated decimal field",
        );
    }
    if fraction_digits > 6 {
        push(
            errors,
            format!("{field_path}.fraction_digits"),
            "must be between 0 and 6",
        );
    }
    let Some(currency) = entity
        .fields
        .iter()
        .find(|candidate| candidate.id == *currency_field)
    else {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "must reference a sibling field",
        );
        return;
    };
    let FieldTypeIr::Enum { values } = &currency.ty else {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "must reference an enum field",
        );
        return;
    };
    if values
        .iter()
        .any(|value| value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "currency enum values must be three uppercase ASCII letters",
        );
    }
    if field.nullable != currency.nullable {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "amount and currency must have matching requiredness",
        );
    }
    if field.read_access != currency.read_access || field.write_access != currency.write_access {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "amount and currency must have identical field access",
        );
    }
    if currency.ui_component.is_some() || currency.ui_semantic.is_some() {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "currency field cannot declare another UI contract",
        );
    }
    if !companions.insert(currency_field) {
        push(
            errors,
            format!("{field_path}.currency_field"),
            "currency field cannot be shared by multiple money fields",
        );
    }
}

fn validate_file_semantic(
    ir: &AppIr,
    field: &FieldIr,
    field_path: &str,
    errors: &mut Vec<IrValidationError>,
) {
    if field.ui_component.is_some() {
        push(
            errors,
            field_path,
            "cannot be combined with a custom UI component",
        );
    }
    if !matches!(field.ty, FieldTypeIr::Uuid) || field.generated.is_some() || field.primary_key {
        push(
            errors,
            field_path,
            "file requires a non-generated, non-primary-key uuid field",
        );
    }
    if !ir.file.enabled || !ir.auth.enabled {
        push(
            errors,
            field_path,
            "file semantic requires enabled auth and file modules",
        );
    }
}

fn validate_display_field(entity: &EntityIr, path: &str, errors: &mut Vec<IrValidationError>) {
    if let Some(id) = &entity.views.display_field
        && !entity.fields.iter().any(|field| {
            field.id == *id
                && matches!(
                    field.ty,
                    FieldTypeIr::String
                        | FieldTypeIr::Text
                        | FieldTypeIr::Enum { .. }
                        | FieldTypeIr::Uuid
                        | FieldTypeIr::Integer
                        | FieldTypeIr::Bigint
                )
        })
    {
        push(
            errors,
            format!("{path}.views.display_field"),
            "must reference a text, enum, UUID or integer field",
        );
    }
}
