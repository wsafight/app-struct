use super::model::SurfaceIndex;
use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_scalar_string, expect_sequence,
    expect_string, required,
};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::{Diagnostic, IndexMethod, IndexOperatorClass};
use std::collections::BTreeMap;

pub(super) fn decode_indexes(node: &Node) -> Result<Vec<SurfaceIndex>, Diagnostic> {
    expect_sequence(node, "entity `indexes`")?
        .iter()
        .map(|item| {
            let mapping = expect_mapping(item, "index definition")?;
            ensure_known_keys(
                mapping,
                &[
                    "name",
                    "fields",
                    "unique",
                    "where",
                    "method",
                    "operator_class",
                ],
                "index definition",
            )?;
            let fields = required(mapping, "fields", &item.span)?;
            let fields = expect_sequence(&fields.value, "index `fields`")?
                .iter()
                .map(|field| expect_string(field, "index field name"))
                .collect::<Result<Vec<_>, _>>()?;
            if fields.is_empty() {
                return Err(Diagnostic::error(
                    "AS1007",
                    "index `fields` must contain at least one field",
                    item.span.clone(),
                ));
            }
            let unique = mapping
                .get("unique")
                .map(|entry| expect_bool(&entry.value, "index `unique`"))
                .transpose()?
                .unwrap_or(false);
            let where_clause = mapping
                .get("where")
                .map(|entry| expect_scalar_string(&entry.value, "index `where`"))
                .transpose()?;
            if where_clause
                .as_ref()
                .is_some_and(|value| value.value.trim().is_empty())
            {
                return Err(Diagnostic::error(
                    "AS1007",
                    "index `where` must not be empty",
                    where_clause.as_ref().expect("checked above").span.clone(),
                ));
            }
            if where_clause.as_ref().is_some_and(|value| {
                value.value.contains(';')
                    || value.value.contains("--")
                    || value.value.contains("/*")
                    || value.value.contains("*/")
            }) {
                return Err(Diagnostic::error(
                    "AS1013",
                    "index `where` cannot contain statement separators or comments",
                    where_clause.as_ref().expect("checked above").span.clone(),
                ));
            }
            Ok(SurfaceIndex {
                method: decode_method(mapping)?,
                operator_class: decode_operator_class(mapping)?,
                name: mapping
                    .get("name")
                    .map(|entry| expect_string(&entry.value, "index `name`"))
                    .transpose()?,
                fields,
                unique,
                where_clause,
                span: item.span.clone(),
            })
        })
        .collect()
}

fn decode_method(mapping: &BTreeMap<String, MappingEntry>) -> Result<IndexMethod, Diagnostic> {
    Ok(
        match mapping
            .get("method")
            .map(|entry| expect_string(&entry.value, "index method"))
            .transpose()?
        {
            None => IndexMethod::Btree,
            Some(value) if value.value == "btree" => IndexMethod::Btree,
            Some(value) if value.value == "gin" => IndexMethod::Gin,
            Some(value) => {
                return Err(Diagnostic::error(
                    "AS1007",
                    "index method must be btree or gin",
                    value.span,
                ));
            }
        },
    )
}

fn decode_operator_class(
    mapping: &BTreeMap<String, MappingEntry>,
) -> Result<Option<IndexOperatorClass>, Diagnostic> {
    Ok(
        match mapping
            .get("operator_class")
            .map(|entry| expect_string(&entry.value, "index operator_class"))
            .transpose()?
        {
            None => None,
            Some(value) if value.value == "gin_trgm_ops" => Some(IndexOperatorClass::GinTrgmOps),
            Some(value) if value.value == "jsonb_ops" => Some(IndexOperatorClass::JsonbOps),
            Some(value) if value.value == "jsonb_path_ops" => {
                Some(IndexOperatorClass::JsonbPathOps)
            }
            Some(value) => {
                return Err(Diagnostic::error(
                    "AS1007",
                    "unsupported index operator_class",
                    value.span,
                ));
            }
        },
    )
}
