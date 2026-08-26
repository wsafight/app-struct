use super::duplicate;
use crate::naming::{is_rust_field_name, is_rust_type_name};
use crate::surface::{SurfaceValueField, SurfaceValueObject};
use appstruct_ir::{Diagnostic, FieldTypeIr, ValueFieldIr, ValueObjectIr};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_value_declarations(
    values: &[SurfaceValueObject],
    entities: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut declarations = BTreeMap::new();
    for value in values {
        if !is_rust_type_name(&value.name.value) {
            diagnostics.push(Diagnostic::error(
                "AS3003",
                format!("invalid value object name `{}`", value.name.value),
                value.name.span.clone(),
            ));
        }
        if entities.contains(&value.name.value) {
            diagnostics.push(Diagnostic::error(
                "AS3004",
                format!("type `{}` conflicts with an entity", value.name.value),
                value.name.span.clone(),
            ));
        }
        if let Some(first) = declarations.insert(value.name.value.clone(), value.name.span.clone())
        {
            diagnostics.push(duplicate(
                "value object",
                &value.name.value,
                &value.name.span,
                first,
            ));
        }
    }
    declarations.into_keys().collect()
}

pub(super) fn lower_value_object(
    value: SurfaceValueObject,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ValueObjectIr> {
    if value.fields.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3005",
            format!("value object `{}` has no fields", value.name.value),
            value.span.clone(),
        ));
        return None;
    }
    let mut names = BTreeMap::new();
    let mut fields = Vec::with_capacity(value.fields.len());
    for field in value.fields {
        if !is_rust_field_name(&field.name.value) {
            diagnostics.push(Diagnostic::error(
                "AS3003",
                format!("invalid value object field `{}`", field.name.value),
                field.name.span.clone(),
            ));
        }
        if let Some(first) = names.insert(field.name.value.clone(), field.name.span.clone()) {
            diagnostics.push(duplicate(
                "value field",
                &field.name.value,
                &field.name.span,
                first,
            ));
        }
        if let Some(ty) = lower_value_type(&field, diagnostics) {
            fields.push(ValueFieldIr {
                rust_name: field.name.value,
                ty,
                required: field.required,
            });
        }
    }
    fields.sort_by(|left, right| left.rust_name.cmp(&right.rust_name));
    Some(ValueObjectIr {
        id: format!("app::{}", value.name.value),
        rust_name: value.name.value,
        fields,
    })
}

fn lower_value_type(
    field: &SurfaceValueField,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FieldTypeIr> {
    let ty = match field.type_name.value.as_str() {
        "uuid" => FieldTypeIr::Uuid,
        "string" => FieldTypeIr::String,
        "text" => FieldTypeIr::Text,
        "integer" => FieldTypeIr::Integer,
        "bigint" => FieldTypeIr::Bigint,
        "decimal" => FieldTypeIr::Decimal,
        "boolean" => FieldTypeIr::Boolean,
        "date" => FieldTypeIr::Date,
        "datetime" => FieldTypeIr::Datetime,
        "json" => FieldTypeIr::Json,
        "enum" => FieldTypeIr::Enum {
            values: field
                .values
                .as_ref()
                .map(|values| values.iter().map(|value| value.value.clone()).collect())
                .unwrap_or_default(),
        },
        other => {
            diagnostics.push(Diagnostic::error(
                "AS3006",
                format!("unsupported value object field type `{other}`"),
                field.type_name.span.clone(),
            ));
            return None;
        }
    };
    if matches!(&ty, FieldTypeIr::Enum { values } if values.is_empty()) {
        diagnostics.push(Diagnostic::error(
            "AS3007",
            "value object enum requires at least one value",
            field.span.clone(),
        ));
    }
    Some(ty)
}
