use super::push;
use crate::{EntityIr, FieldTypeIr, IndexMethod, IndexOperatorClass};
use std::collections::BTreeSet;

pub(super) fn validate_indexes(
    entity: &EntityIr,
    path: &str,
    errors: &mut Vec<super::IrValidationError>,
) {
    let field_set = entity
        .fields
        .iter()
        .map(|field| field.id.0.as_str())
        .collect::<BTreeSet<_>>();
    let mut index_ids = BTreeSet::new();
    for (index_index, index) in entity.indexes.iter().enumerate() {
        let index_path = format!("{path}.indexes[{index_index}]");
        let compatible = match (index.method, index.operator_class) {
            (IndexMethod::Btree, None) => true,
            (IndexMethod::Gin, Some(class)) if !index.unique => index.fields.iter().all(|id| {
                entity
                    .fields
                    .iter()
                    .find(|field| field.id == *id)
                    .is_some_and(|field| match class {
                        IndexOperatorClass::GinTrgmOps => matches!(
                            field.ty,
                            FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. }
                        ),
                        IndexOperatorClass::JsonbOps | IndexOperatorClass::JsonbPathOps => {
                            matches!(field.ty, FieldTypeIr::Json)
                        }
                    })
            }),
            _ => false,
        };
        if !compatible {
            push(
                errors,
                format!("{index_path}.method"),
                "index method/operator_class is incompatible with its fields or uniqueness",
            );
        }
        if index.entity != entity.id {
            push(
                errors,
                format!("{index_path}.entity"),
                format!("must reference containing entity `{}`", entity.id),
            );
        }
        if !index_ids.insert(index.id.as_str()) {
            push(
                errors,
                format!("{index_path}.id"),
                format!("duplicate index id `{}`", index.id),
            );
        }
        if index.fields.is_empty() {
            push(errors, format!("{index_path}.fields"), "must not be empty");
        }
        let mut fields = BTreeSet::new();
        for (field_index, field) in index.fields.iter().enumerate() {
            if !field_set.contains(field.0.as_str()) {
                push(
                    errors,
                    format!("{index_path}.fields[{field_index}]"),
                    format!("references missing field `{field}`"),
                );
            }
            if !fields.insert(field.0.as_str()) {
                push(
                    errors,
                    format!("{index_path}.fields[{field_index}]"),
                    format!("lists field `{field}` more than once"),
                );
            }
        }
        if index.predicate.as_deref().is_some_and(|value| {
            value.contains(';')
                || value.contains("--")
                || value.contains("/*")
                || value.contains("*/")
        }) {
            push(
                errors,
                format!("{index_path}.predicate"),
                "must not contain statement separators or comments",
            );
        }
    }
}
