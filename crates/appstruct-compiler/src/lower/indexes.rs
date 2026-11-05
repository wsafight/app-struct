use super::super::surface::SurfaceEntity;
use appstruct_ir::{
    Diagnostic, EntityId, FieldIr, FieldTypeIr, IndexIr, IndexMethod, IndexOperatorClass,
};
use std::collections::BTreeSet;

/// PostgreSQL truncates identifiers beyond this many bytes. Generated names must fit untruncated
/// so two long names cannot collapse into one index.
const MAX_IDENTIFIER_BYTES: usize = 63;

/// Adds `(tenant_id, <sortable field>, <primary key>)` to `indexes` for a tenant-scoped entity.
///
/// Tenant-scoped list queries filter on `tenant_id` and order by the requested field with the
/// primary key as tiebreaker, so the equality column leads and the sort columns follow. The
/// project switch `modules.tenant.sortable_indexes` applies to every tenant-scoped entity unless
/// the entity sets `tenant_index` itself. Soft-deleted entities get a partial index because every
/// generated read already filters `deleted_at IS NULL`.
fn add_tenant_sort_indexes(
    entity: &SurfaceEntity,
    entity_id: &EntityId,
    fields: &[FieldIr],
    project_default: bool,
    indexes: &mut Vec<IndexIr>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(requested) = &entity.tenant_index
        && requested.value
        && !entity.tenant_scoped
    {
        diagnostics.push(Diagnostic::error(
            "AS2065",
            format!(
                "entity `{}` sets `tenant_index` but is not tenant-scoped; add `tenant: true` or remove it",
                entity.name.value
            ),
            requested.span.clone(),
        ));
        return;
    }
    let enabled = entity
        .tenant_index
        .as_ref()
        .map_or(project_default, |requested| requested.value);
    if !enabled || !entity.tenant_scoped {
        return;
    }
    let (Some(tenant), Some(primary)) = (
        fields.iter().find(|field| field.rust_name == "tenant_id"),
        fields.iter().find(|field| field.primary_key),
    ) else {
        return;
    };
    let predicate = entity.soft_delete.then(|| {
        let column = fields
            .iter()
            .find(|field| field.rust_name == "deleted_at")
            .map_or("deleted_at", |field| field.column_name.as_str());
        if column == "deleted_at" {
            "deleted_at IS NULL".to_owned()
        } else {
            format!("\"{column}\" IS NULL")
        }
    });
    for field in fields.iter().filter(|field| {
        field.capabilities.sortable && !field.primary_key && field.rust_name != "tenant_id"
    }) {
        let columns = vec![tenant.id.clone(), field.id.clone(), primary.id.clone()];
        if indexes.iter().any(|existing| {
            existing.fields == columns
                && existing.method == IndexMethod::Btree
                && existing.predicate == predicate
        }) {
            continue;
        }
        // The SQL name is `idx_` plus everything after the last `::`, so it carries the entity.
        let suffix = format!("{}_tenant_sort_{}", entity.name.value, field.rust_name);
        if suffix.len() + "idx_".len() > MAX_IDENTIFIER_BYTES {
            diagnostics.push(Diagnostic::error(
                "AS2066",
                format!(
                    "generated tenant index name for `{}.{}` exceeds {MAX_IDENTIFIER_BYTES} bytes; declare it by hand under `indexes` with a shorter name",
                    entity.name.value, field.rust_name
                ),
                entity.span.clone(),
            ));
            continue;
        }
        let id = format!("app::{}::{suffix}", entity.name.value);
        indexes.push(IndexIr {
            id,
            entity: entity_id.clone(),
            fields: columns,
            unique: false,
            method: IndexMethod::Btree,
            operator_class: None,
            predicate: predicate.clone(),
        });
    }
}

/// Declared indexes plus, when enabled, the generated tenant sort indexes.
pub(super) fn build_indexes(
    entity: &SurfaceEntity,
    entity_id: &EntityId,
    fields: &[FieldIr],
    tenant_sortable_indexes: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<IndexIr> {
    let mut indexes = build_declared_indexes(entity, entity_id, fields, diagnostics);
    add_tenant_sort_indexes(
        entity,
        entity_id,
        fields,
        tenant_sortable_indexes,
        &mut indexes,
        diagnostics,
    );
    indexes
}

fn build_declared_indexes(
    entity: &SurfaceEntity,
    entity_id: &EntityId,
    fields: &[FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<IndexIr> {
    let mut indexes = Vec::with_capacity(entity.indexes.len());
    let mut ids = BTreeSet::new();
    for index in &entity.indexes {
        let id = index.name.as_ref().map_or_else(
            || {
                format!(
                    "app::{}.auto_index_{}",
                    entity.name.value,
                    index
                        .fields
                        .iter()
                        .map(|field| field.value.as_str())
                        .collect::<Vec<_>>()
                        .join("_")
                )
            },
            |name| format!("app::{}::{}", entity.name.value, name.value),
        );
        if !ids.insert(id.clone()) {
            diagnostics.push(Diagnostic::error(
                "AS2050",
                format!("index `{id}` is declared more than once"),
                index.span.clone(),
            ));
            continue;
        }
        if let Some(name) = &index.name
            && !appstruct_core::is_sql_name(&name.value)
        {
            diagnostics.push(Diagnostic::error(
                "AS2051",
                format!("invalid index name `{}`", name.value),
                name.span.clone(),
            ));
            continue;
        }
        let mut field_ids = Vec::with_capacity(index.fields.len());
        let mut seen = BTreeSet::new();
        for field in &index.fields {
            let Some(found) = fields.iter().find(|candidate| {
                candidate.api_name == field.value || candidate.rust_name == field.value
            }) else {
                diagnostics.push(Diagnostic::error(
                    "AS2052",
                    format!("index `{id}` references unknown field `{}`", field.value),
                    field.span.clone(),
                ));
                continue;
            };
            if !seen.insert(found.id.clone()) {
                diagnostics.push(Diagnostic::error(
                    "AS2053",
                    format!("index `{id}` lists field `{}` more than once", field.value),
                    field.span.clone(),
                ));
                continue;
            }
            field_ids.push(found.id.clone());
        }
        if field_ids.len() != index.fields.len() || field_ids.is_empty() {
            continue;
        }
        let compatible = match (index.method, index.operator_class) {
            (IndexMethod::Btree, None) => true,
            (IndexMethod::Gin, Some(class)) if !index.unique => fields
                .iter()
                .filter(|field| field_ids.contains(&field.id))
                .all(|field| match class {
                    IndexOperatorClass::GinTrgmOps => matches!(
                        field.ty,
                        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. }
                    ),
                    IndexOperatorClass::JsonbOps | IndexOperatorClass::JsonbPathOps => {
                        matches!(field.ty, FieldTypeIr::Json)
                    }
                }),
            _ => false,
        };
        if !compatible {
            diagnostics.push(Diagnostic::error("AS2054", "GIN indexes require a compatible operator_class and cannot be unique; B-tree indexes do not accept operator_class", index.span.clone()));
            continue;
        }
        indexes.push(IndexIr {
            id,
            entity: entity_id.clone(),
            fields: field_ids,
            unique: index.unique,
            method: index.method,
            operator_class: index.operator_class,
            predicate: index.where_clause.as_ref().map(|value| value.value.clone()),
        });
    }
    indexes
}
