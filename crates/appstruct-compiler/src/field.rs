use crate::naming::{is_rust_field_name, is_sql_name};
use crate::surface::SurfaceField;
use appstruct_ir::{
    Cardinality, Diagnostic, EntityId, FieldId, FieldTypeIr, GeneratedValueIr, OnDeleteIr,
    RelationId, RelationIr, SourceSpan,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn build_column(
    field: &SurfaceField,
    columns: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(String, bool)> {
    if !is_rust_field_name(&field.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            format!("invalid field name `{}`", field.name.value),
            field.name.span.clone(),
        ));
        return None;
    }
    let is_relation = field.type_name.value == "relation";
    let default_column = if is_relation {
        format!("{}_id", field.name.value)
    } else {
        field.name.value.clone()
    };
    let column_name = field
        .column
        .as_ref()
        .map_or(default_column, |column| column.value.clone());
    let column_span = field
        .column
        .as_ref()
        .map_or_else(|| field.name.span.clone(), |column| column.span.clone());
    if !is_sql_name(&column_name) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            format!("invalid column name `{column_name}`"),
            column_span.clone(),
        ));
    }
    if let Some(first) = columns.insert(column_name.clone(), column_span.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "AS2005",
                format!("column `{column_name}` is used by multiple fields"),
                column_span,
            )
            .with_secondary(first, "first used here"),
        );
    }
    Some((column_name, is_relation))
}

pub(crate) fn build_relation(
    field: &SurfaceField,
    entity_id: &EntityId,
    field_id: &FieldId,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<RelationIr> {
    let FieldTypeIr::Relation { target } = field_type else {
        return None;
    };
    let on_delete = build_on_delete(field, diagnostics);
    if field.flags.required() && on_delete == OnDeleteIr::SetNull {
        diagnostics.push(Diagnostic::error(
            "AS2006",
            "a required relation cannot use `on_delete: set_null`",
            field
                .on_delete
                .as_ref()
                .map_or_else(|| field.span.clone(), |value| value.span.clone()),
        ));
    }
    Some(RelationIr {
        id: RelationId(field_id.0.clone()),
        source: entity_id.clone(),
        target: target.clone(),
        cardinality: if field.flags.unique() {
            Cardinality::OneToOne
        } else {
            Cardinality::ManyToOne
        },
        foreign_key_owner: entity_id.clone(),
        foreign_key_fields: vec![field_id.clone()],
        inverse: None,
        required: field.flags.required(),
        unique: field.flags.unique(),
        on_delete,
    })
}

pub(crate) fn build_field_type(
    field: &SurfaceField,
    known_entities: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FieldTypeIr> {
    let scalar = match field.type_name.value.as_str() {
        "uuid" => Some(FieldTypeIr::Uuid),
        "string" => Some(FieldTypeIr::String),
        "text" => Some(FieldTypeIr::Text),
        "integer" => Some(FieldTypeIr::Integer),
        "bigint" => Some(FieldTypeIr::Bigint),
        "decimal" => Some(FieldTypeIr::Decimal),
        "boolean" => Some(FieldTypeIr::Boolean),
        "date" => Some(FieldTypeIr::Date),
        "datetime" => Some(FieldTypeIr::Datetime),
        "json" => Some(FieldTypeIr::Json),
        "enum" | "relation" => None,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "AS2007",
                    format!("unsupported field type `{other}`"),
                    field.type_name.span.clone(),
                )
                .with_help("use uuid, string, text, integer, bigint, decimal, boolean, date, datetime, json, enum, or relation"),
            );
            return None;
        }
    };
    if let Some(scalar) = scalar {
        return Some(scalar);
    }
    if field.type_name.value == "enum" {
        return build_enum_type(field, diagnostics);
    }
    build_relation_type(field, known_entities, diagnostics)
}

fn build_enum_type(field: &SurfaceField, diagnostics: &mut Vec<Diagnostic>) -> Option<FieldTypeIr> {
    let Some(values) = &field.values else {
        diagnostics.push(Diagnostic::error(
            "AS2008",
            "enum fields require a non-empty `values` sequence",
            field.span.clone(),
        ));
        return None;
    };
    if values.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS2008",
            "enum fields require a non-empty `values` sequence",
            field.span.clone(),
        ));
        return None;
    }
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value.value.clone()) {
            diagnostics.push(Diagnostic::error(
                "AS2009",
                format!("duplicate enum value `{}`", value.value),
                value.span.clone(),
            ));
        }
    }
    Some(FieldTypeIr::Enum {
        values: values.iter().map(|value| value.value.clone()).collect(),
    })
}

fn build_relation_type(
    field: &SurfaceField,
    known_entities: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FieldTypeIr> {
    let Some(target) = &field.target else {
        diagnostics.push(Diagnostic::error(
            "AS2010",
            "relation fields require `target`",
            field.span.clone(),
        ));
        return None;
    };
    let target_name = target.value.strip_prefix("app::").unwrap_or(&target.value);
    if target.value.contains("::") && !target.value.starts_with("app::") {
        diagnostics.push(
            Diagnostic::error(
                "AS2011",
                format!(
                    "module relation target `{}` is not available in M0",
                    target.value
                ),
                target.span.clone(),
            )
            .with_help("use an application entity or wait for module IR fragments"),
        );
        return None;
    }
    if !known_entities.contains(target_name) {
        diagnostics.push(Diagnostic::error(
            "AS2011",
            format!("unknown relation target `{}`", target.value),
            target.span.clone(),
        ));
        return None;
    }
    Some(FieldTypeIr::Relation {
        target: EntityId(format!("app::{target_name}")),
    })
}

pub(crate) fn validate_field_options(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let is_text = matches!(field_type, FieldTypeIr::String | FieldTypeIr::Text);
    if !is_text && (field.min_length.is_some() || field.max_length.is_some()) {
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
    if let (Some(default), FieldTypeIr::Enum { values }) = (&field.default, field_type)
        && !values.contains(&default.value)
    {
        diagnostics.push(Diagnostic::error(
            "AS2014",
            format!(
                "enum default `{}` is not one of its declared values",
                default.value
            ),
            default.span.clone(),
        ));
    }
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

fn build_on_delete(field: &SurfaceField, diagnostics: &mut Vec<Diagnostic>) -> OnDeleteIr {
    let Some(on_delete) = &field.on_delete else {
        return OnDeleteIr::Restrict;
    };
    match on_delete.value.as_str() {
        "restrict" => OnDeleteIr::Restrict,
        "cascade" => OnDeleteIr::Cascade,
        "set_null" => OnDeleteIr::SetNull,
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS2016",
                format!("unknown `on_delete` policy `{}`", on_delete.value),
                on_delete.span.clone(),
            ));
            OnDeleteIr::Restrict
        }
    }
}
