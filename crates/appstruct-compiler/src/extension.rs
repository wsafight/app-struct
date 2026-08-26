use crate::access::convert_rule;
use crate::naming::{is_rust_field_name, is_rust_type_name};
use crate::surface::{SurfaceOperation, SurfacePage, SurfaceValueField, SurfaceValueObject};
use appstruct_ir::{
    CommandIr, Diagnostic, EntityId, FieldTypeIr, OperationTypeIr, PageIr, QueryIr, SourceSpan,
    ValueFieldIr, ValueObjectIr,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct LoweredExtensions {
    pub value_objects: Vec<ValueObjectIr>,
    pub commands: Vec<CommandIr>,
    pub queries: Vec<QueryIr>,
    pub pages: Vec<PageIr>,
}

pub(crate) fn lower_extensions(
    value_objects: Vec<SurfaceValueObject>,
    commands: Vec<SurfaceOperation>,
    queries: Vec<SurfaceOperation>,
    pages: Vec<SurfacePage>,
    known_entities: &BTreeSet<String>,
    resource_paths: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> LoweredExtensions {
    let known_values = validate_value_declarations(&value_objects, known_entities, diagnostics);
    let mut lowered_values = value_objects
        .into_iter()
        .filter_map(|value| lower_value_object(value, diagnostics))
        .collect::<Vec<_>>();
    let mut operation_names = BTreeMap::new();
    let mut lowered_commands = commands
        .into_iter()
        .filter_map(|operation| {
            validate_operation_name(&operation, &mut operation_names, diagnostics);
            lower_command(operation, known_entities, &known_values, diagnostics)
        })
        .collect::<Vec<_>>();
    let mut lowered_queries = queries
        .into_iter()
        .filter_map(|operation| {
            validate_operation_name(&operation, &mut operation_names, diagnostics);
            lower_query(operation, known_entities, &known_values, diagnostics)
        })
        .collect::<Vec<_>>();
    let mut lowered_pages = lower_pages(pages, resource_paths, diagnostics);
    lowered_values.sort_by(|left, right| left.id.cmp(&right.id));
    lowered_commands.sort_by(|left, right| left.id.cmp(&right.id));
    lowered_queries.sort_by(|left, right| left.id.cmp(&right.id));
    lowered_pages.sort_by(|left, right| left.id.cmp(&right.id));
    LoweredExtensions {
        value_objects: lowered_values,
        commands: lowered_commands,
        queries: lowered_queries,
        pages: lowered_pages,
    }
}

fn validate_value_declarations(
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

fn lower_value_object(
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

fn lower_command(
    operation: SurfaceOperation,
    entities: &BTreeSet<String>,
    values: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CommandIr> {
    let input = resolve_type(operation.input.as_ref()?, entities, values, diagnostics)?;
    let output = resolve_type(&operation.output, entities, values, diagnostics)?;
    Some(CommandIr {
        id: format!("app::command::{}", operation.name.value),
        rust_name: operation.name.value,
        input,
        output,
        access: convert_rule(&operation.access),
    })
}

fn lower_query(
    operation: SurfaceOperation,
    entities: &BTreeSet<String>,
    values: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<QueryIr> {
    let input = operation
        .input
        .as_ref()
        .and_then(|input| resolve_type(input, entities, values, diagnostics));
    let output = resolve_type(&operation.output, entities, values, diagnostics)?;
    Some(QueryIr {
        id: format!("app::query::{}", operation.name.value),
        rust_name: operation.name.value,
        input,
        output,
        access: convert_rule(&operation.access),
    })
}

fn resolve_type(
    reference: &crate::surface::Located<String>,
    entities: &BTreeSet<String>,
    values: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<OperationTypeIr> {
    if entities.contains(&reference.value) {
        return Some(OperationTypeIr::Entity {
            entity: EntityId(format!("app::{}", reference.value)),
        });
    }
    if values.contains(&reference.value) {
        return Some(OperationTypeIr::ValueObject {
            value_object: format!("app::{}", reference.value),
        });
    }
    diagnostics.push(Diagnostic::error(
        "AS3008",
        format!("unknown operation type `{}`", reference.value),
        reference.span.clone(),
    ));
    None
}

fn validate_operation_name(
    operation: &SurfaceOperation,
    declarations: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_rust_type_name(&operation.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS3003",
            format!("invalid operation name `{}`", operation.name.value),
            operation.name.span.clone(),
        ));
    }
    if let Some(first) =
        declarations.insert(operation.name.value.clone(), operation.name.span.clone())
    {
        diagnostics.push(duplicate(
            "operation",
            &operation.name.value,
            &operation.span,
            first,
        ));
    }
}

fn lower_pages(
    pages: Vec<SurfacePage>,
    resource_paths: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<PageIr> {
    let mut names = BTreeMap::new();
    let mut paths = BTreeMap::new();
    pages
        .into_iter()
        .map(|page| {
            if !is_rust_type_name(&page.name.value) || !is_rust_type_name(&page.component.value) {
                diagnostics.push(Diagnostic::error(
                    "AS3003",
                    "page and component names must be PascalCase identifiers",
                    page.span.clone(),
                ));
            }
            if !valid_page_path(&page.path.value) {
                diagnostics.push(Diagnostic::error(
                    "AS3009",
                    format!("invalid page path `{}`", page.path.value),
                    page.path.span.clone(),
                ));
            }
            if page.path.value == "empty"
                || resource_paths.iter().any(|resource| {
                    page.path.value == *resource
                        || page.path.value.starts_with(&format!("{resource}/"))
                })
            {
                diagnostics.push(Diagnostic::error(
                    "AS3012",
                    format!(
                        "page path `{}` conflicts with a generated route",
                        page.path.value
                    ),
                    page.path.span.clone(),
                ));
            }
            if let Some(first) = names.insert(page.name.value.clone(), page.name.span.clone()) {
                diagnostics.push(duplicate("page", &page.name.value, &page.name.span, first));
            }
            if let Some(first) = paths.insert(page.path.value.clone(), page.path.span.clone()) {
                diagnostics.push(duplicate(
                    "page path",
                    &page.path.value,
                    &page.path.span,
                    first,
                ));
            }
            PageIr {
                id: format!("app::page::{}", page.name.value),
                rust_name: page.name.value.clone(),
                label: page.label.map_or(page.name.value, |label| label.value),
                path: page.path.value,
                component: page.component.value,
            }
        })
        .collect()
}

fn valid_page_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && value.split('/').all(|segment| {
            segment
                .bytes()
                .next()
                .is_some_and(|first| first.is_ascii_lowercase())
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn duplicate(kind: &str, name: &str, span: &SourceSpan, first: SourceSpan) -> Diagnostic {
    Diagnostic::error(
        "AS3010",
        format!("{kind} `{name}` is declared more than once"),
        span.clone(),
    )
    .with_secondary(first, "first declared here")
}
