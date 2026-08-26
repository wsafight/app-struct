use crate::naming::{is_app_name, is_rust_type_name, is_sql_name, pluralize, to_snake_case};
use crate::surface::{SurfaceEntity, SurfaceRoot};
use appstruct_ir::{Diagnostic, SourceSpan};
use std::collections::BTreeMap;

pub(crate) fn validate_root(root: &SurfaceRoot) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if root.version.value != 1 {
        diagnostics.push(
            Diagnostic::error(
                "AS1011",
                format!("unsupported App Spec version `{}`", root.version.value),
                root.version.span.clone(),
            )
            .with_help("this compiler supports App Spec version 1"),
        );
    }
    if !is_app_name(&root.app_name.value) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            "app name must start with a lowercase ASCII letter and contain only lowercase letters, digits, or hyphens",
            root.app_name.span.clone(),
        ));
    }
    if root.database_provider.value != "postgres" {
        diagnostics.push(
            Diagnostic::error(
                "AS4001",
                format!(
                    "unsupported database provider `{}`",
                    root.database_provider.value
                ),
                root.database_provider.span.clone(),
            )
            .with_help("M0 supports only `postgres`"),
        );
    }
    if !matches!(root.database_mode.value.as_str(), "managed" | "external") {
        diagnostics.push(Diagnostic::error(
            "AS4002",
            format!(
                "unsupported database dev mode `{}`",
                root.database_mode.value
            ),
            root.database_mode.span.clone(),
        ));
    }
    diagnostics
}

pub(crate) fn validate_entity_declarations(surface_entities: &[SurfaceEntity]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut entity_declarations = BTreeMap::<String, SourceSpan>::new();
    let mut table_declarations = BTreeMap::<String, SourceSpan>::new();
    for entity in surface_entities {
        validate_entity_name(entity, &mut entity_declarations, &mut diagnostics);
        validate_table_name(entity, &mut table_declarations, &mut diagnostics);
    }
    diagnostics
}

fn validate_entity_name(
    entity: &SurfaceEntity,
    declarations: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_rust_type_name(&entity.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            format!("invalid entity name `{}`", entity.name.value),
            entity.name.span.clone(),
        ));
    }
    if let Some(first) = declarations.insert(entity.name.value.clone(), entity.name.span.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "AS2002",
                format!("entity `{}` is declared more than once", entity.name.value),
                entity.name.span.clone(),
            )
            .with_secondary(first, "first declared here"),
        );
    }
}

fn validate_table_name(
    entity: &SurfaceEntity,
    declarations: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let table_name = entity.table.as_ref().map_or_else(
        || pluralize(&to_snake_case(&entity.name.value)),
        |table| table.value.clone(),
    );
    let table_span = entity
        .table
        .as_ref()
        .map_or_else(|| entity.name.span.clone(), |table| table.span.clone());
    if !is_sql_name(&table_name) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            format!("invalid table name `{table_name}`"),
            table_span.clone(),
        ));
    }
    if let Some(first) = declarations.insert(table_name.clone(), table_span.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "AS2003",
                format!("table `{table_name}` is used by multiple entities"),
                table_span,
            )
            .with_secondary(first, "first used here"),
        );
    }
}

pub(crate) fn validate_primary_key(entity: &SurfaceEntity, diagnostics: &mut Vec<Diagnostic>) {
    let count = entity
        .fields
        .iter()
        .filter(|field| field.flags.primary_key())
        .count();
    if count != 1 {
        diagnostics.push(Diagnostic::error(
            "AS2004",
            format!(
                "entity `{}` must define exactly one primary key; found {count}",
                entity.name.value
            ),
            entity.span.clone(),
        ));
    }
}
