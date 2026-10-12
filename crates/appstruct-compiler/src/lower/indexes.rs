use super::super::surface::SurfaceEntity;
use appstruct_ir::{
    Diagnostic, EntityId, FieldIr, FieldTypeIr, IndexIr, IndexMethod, IndexOperatorClass,
};
use std::collections::BTreeSet;

pub(super) fn build_indexes(
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
