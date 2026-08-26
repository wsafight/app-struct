mod value;

use crate::access::build_operation_access;
use crate::naming::is_rust_type_name;
use crate::surface::{SurfaceOperation, SurfacePage, SurfaceValueObject};
use appstruct_ir::{
    AuthIr, CommandIr, Diagnostic, EntityId, OperationTypeIr, PageIr, QueryIr, SourceSpan,
    ValueObjectIr,
};
use std::collections::{BTreeMap, BTreeSet};

use value::{lower_value_object, validate_value_declarations};

pub(crate) struct LoweredExtensions {
    pub value_objects: Vec<ValueObjectIr>,
    pub commands: Vec<CommandIr>,
    pub queries: Vec<QueryIr>,
    pub pages: Vec<PageIr>,
}

pub(crate) struct ExtensionContext<'context> {
    pub known_entities: &'context BTreeSet<String>,
    pub resource_paths: &'context BTreeSet<String>,
    pub auth: &'context AuthIr,
}

pub(crate) fn lower_extensions(
    value_objects: Vec<SurfaceValueObject>,
    commands: Vec<SurfaceOperation>,
    queries: Vec<SurfaceOperation>,
    pages: Vec<SurfacePage>,
    context: &ExtensionContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> LoweredExtensions {
    let known_values =
        validate_value_declarations(&value_objects, context.known_entities, diagnostics);
    let mut lowered_values = value_objects
        .into_iter()
        .filter_map(|value| lower_value_object(value, diagnostics))
        .collect::<Vec<_>>();
    let mut operation_names = BTreeMap::new();
    let mut lowered_commands = commands
        .into_iter()
        .filter_map(|operation| {
            validate_operation_name(&operation, &mut operation_names, diagnostics);
            lower_command(
                operation,
                context.known_entities,
                &known_values,
                context.auth,
                diagnostics,
            )
        })
        .collect::<Vec<_>>();
    let mut lowered_queries = queries
        .into_iter()
        .filter_map(|operation| {
            validate_operation_name(&operation, &mut operation_names, diagnostics);
            lower_query(
                operation,
                context.known_entities,
                &known_values,
                context.auth,
                diagnostics,
            )
        })
        .collect::<Vec<_>>();
    let mut lowered_pages = lower_pages(pages, context.resource_paths, diagnostics);
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

fn lower_command(
    operation: SurfaceOperation,
    entities: &BTreeSet<String>,
    values: &BTreeSet<String>,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CommandIr> {
    let input = resolve_type(operation.input.as_ref()?, entities, values, diagnostics)?;
    let output = resolve_type(&operation.output, entities, values, diagnostics)?;
    Some(CommandIr {
        id: format!("app::command::{}", operation.name.value),
        rust_name: operation.name.value,
        input,
        output,
        access: build_operation_access(&operation.access, auth, diagnostics)?,
    })
}

fn lower_query(
    operation: SurfaceOperation,
    entities: &BTreeSet<String>,
    values: &BTreeSet<String>,
    auth: &AuthIr,
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
        access: build_operation_access(&operation.access, auth, diagnostics)?,
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
