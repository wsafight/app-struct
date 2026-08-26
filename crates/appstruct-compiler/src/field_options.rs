use crate::surface::SurfaceField;
use appstruct_ir::{Diagnostic, FieldTypeIr, GeneratedValueIr};

pub(crate) fn validate_field_options(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_length(field, field_type, diagnostics);
    validate_numeric_bounds(field, field_type, diagnostics);
    validate_type_specific_options(field, field_type, diagnostics);
    validate_default(field, field_type, diagnostics);
    if field.flags.primary_key()
        && !matches!(
            field_type,
            FieldTypeIr::Uuid | FieldTypeIr::Integer | FieldTypeIr::Bigint
        )
    {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "primary keys must use uuid, integer, or bigint",
            field.span.clone(),
        ));
    }
}

fn validate_length(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !matches!(field_type, FieldTypeIr::String | FieldTypeIr::Text)
        && (field.min_length.is_some() || field.max_length.is_some())
    {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`min_length` and `max_length` are valid only for string or text fields",
            field.span.clone(),
        ));
    }
    if let (Some(minimum), Some(maximum)) = (&field.min_length, &field.max_length)
        && minimum.value > maximum.value
    {
        diagnostics.push(
            Diagnostic::error(
                "AS2013",
                "`min_length` cannot be greater than `max_length`",
                minimum.span.clone(),
            )
            .with_secondary(maximum.span.clone(), "maximum declared here"),
        );
    }
}

fn validate_numeric_bounds(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let numeric = matches!(
        field_type,
        FieldTypeIr::Integer | FieldTypeIr::Bigint | FieldTypeIr::Decimal
    );
    if !numeric && (field.minimum.is_some() || field.maximum.is_some()) {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`minimum` and `maximum` are valid only for numeric fields",
            field.span.clone(),
        ));
        return;
    }
    match field_type {
        FieldTypeIr::Integer => validate_parsed_bounds::<i32>(field, diagnostics),
        FieldTypeIr::Bigint => validate_parsed_bounds::<i64>(field, diagnostics),
        FieldTypeIr::Decimal => validate_parsed_bounds::<f64>(field, diagnostics),
        _ => {}
    }
}

fn validate_parsed_bounds<T>(field: &SurfaceField, diagnostics: &mut Vec<Diagnostic>)
where
    T: std::str::FromStr + PartialOrd,
{
    let parse = |value: &str| value.parse::<T>().ok();
    let minimum = field.minimum.as_ref().and_then(|value| parse(&value.value));
    let maximum = field.maximum.as_ref().and_then(|value| parse(&value.value));
    for bound in [field.minimum.as_ref(), field.maximum.as_ref()]
        .into_iter()
        .flatten()
    {
        if parse(&bound.value).is_none() {
            diagnostics.push(Diagnostic::error(
                "AS2017",
                format!(
                    "numeric bound `{}` is invalid for this field type",
                    bound.value
                ),
                bound.span.clone(),
            ));
        }
    }
    if let (Some(minimum), Some(maximum)) = (minimum, maximum)
        && minimum > maximum
    {
        diagnostics.push(Diagnostic::error(
            "AS2018",
            "`minimum` cannot be greater than `maximum`",
            field.minimum.as_ref().expect("present").span.clone(),
        ));
    }
}

fn validate_type_specific_options(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !matches!(field_type, FieldTypeIr::Enum { .. }) && field.values.is_some() {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`values` is valid only for enum fields",
            field.span.clone(),
        ));
    }
    if !matches!(field_type, FieldTypeIr::Relation { .. })
        && (field.target.is_some() || field.on_delete.is_some())
    {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`target` and `on_delete` are valid only for relation fields",
            field.span.clone(),
        ));
    }
    if field.flags.searchable() && !matches!(field_type, FieldTypeIr::String | FieldTypeIr::Text) {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`searchable` is valid only for string or text fields",
            field.span.clone(),
        ));
    }
    if matches!(field_type, FieldTypeIr::Json)
        && (field.flags.filterable() || field.flags.sortable())
    {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "json fields cannot use default filtering or sorting",
            field.span.clone(),
        ));
    }
}

fn validate_default(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(default) = &field.default else {
        return;
    };
    let valid = match field_type {
        FieldTypeIr::Enum { values } => values.contains(&default.value),
        FieldTypeIr::Integer => default.value.parse::<i32>().is_ok(),
        FieldTypeIr::Bigint => default.value.parse::<i64>().is_ok(),
        FieldTypeIr::Decimal => default.value.parse::<f64>().is_ok_and(f64::is_finite),
        FieldTypeIr::Boolean => default.value.parse::<bool>().is_ok(),
        FieldTypeIr::Relation { .. } => false,
        _ => true,
    };
    if !valid {
        diagnostics.push(Diagnostic::error(
            "AS2014",
            format!("default `{}` is invalid for this field type", default.value),
            default.span.clone(),
        ));
    }
    if field.generated.is_some() {
        diagnostics.push(Diagnostic::error(
            "AS2019",
            "a field cannot declare both `default` and `generated`",
            default.span.clone(),
        ));
    }
}

pub(crate) fn build_generated(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<GeneratedValueIr> {
    let generated = field.generated.as_ref()?;
    let value = match generated.value.as_str() {
        "uuid_v7" if matches!(field_type, FieldTypeIr::Uuid) => GeneratedValueIr::UuidV7,
        "now" if matches!(field_type, FieldTypeIr::Date | FieldTypeIr::Datetime) => {
            GeneratedValueIr::Now
        }
        "auto_increment" if matches!(field_type, FieldTypeIr::Integer | FieldTypeIr::Bigint) => {
            GeneratedValueIr::AutoIncrement
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS2015",
                format!(
                    "generated value `{}` is incompatible with field type `{}`",
                    generated.value, field.type_name.value
                ),
                generated.span.clone(),
            ));
            return None;
        }
    };
    Some(value)
}
