use super::access::decode_rule;
use super::value::{
    ensure_known_keys, expect_mapping, expect_sequence, expect_string, optional_bool,
    optional_string, required,
};
use super::{Located, SurfaceAccessRule};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::{Diagnostic, SourceSpan};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct SurfaceValueObject {
    pub name: Located<String>,
    pub fields: Vec<SurfaceValueField>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceValueField {
    pub name: Located<String>,
    pub type_name: Located<String>,
    pub required: bool,
    pub values: Option<Vec<Located<String>>>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceOperation {
    pub name: Located<String>,
    pub input: Option<Located<String>>,
    pub output: Located<String>,
    pub access: Located<SurfaceAccessRule>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfacePage {
    pub name: Located<String>,
    pub label: Option<Located<String>>,
    pub path: Located<String>,
    pub component: Located<String>,
    pub span: SourceSpan,
}

pub(super) fn decode_value_objects(
    domain: &BTreeMap<String, MappingEntry>,
) -> Result<Vec<SurfaceValueObject>, Vec<Diagnostic>> {
    let Some(node) = domain.get("value_objects") else {
        return Ok(Vec::new());
    };
    let definitions =
        expect_mapping(&node.value, "`value_objects`").map_err(|error| vec![error])?;
    collect_definitions(definitions, decode_value_object)
}

pub(super) fn decode_operations(
    domain: &BTreeMap<String, MappingEntry>,
    key: &str,
) -> Result<Vec<SurfaceOperation>, Vec<Diagnostic>> {
    let Some(node) = domain.get(key) else {
        return Ok(Vec::new());
    };
    let definitions =
        expect_mapping(&node.value, &format!("`{key}`")).map_err(|error| vec![error])?;
    collect_definitions(definitions, |name, entry| {
        decode_operation(name, entry, key)
    })
}

pub(super) fn decode_pages(
    domain: &BTreeMap<String, MappingEntry>,
) -> Result<Vec<SurfacePage>, Vec<Diagnostic>> {
    let Some(node) = domain.get("pages") else {
        return Ok(Vec::new());
    };
    let definitions = expect_mapping(&node.value, "`pages`").map_err(|error| vec![error])?;
    collect_definitions(definitions, decode_page)
}

fn collect_definitions<T>(
    definitions: &BTreeMap<String, MappingEntry>,
    mut decode: impl FnMut(&str, &MappingEntry) -> Result<T, Diagnostic>,
) -> Result<Vec<T>, Vec<Diagnostic>> {
    let mut values = Vec::with_capacity(definitions.len());
    let mut diagnostics = Vec::new();
    for (name, entry) in definitions {
        match decode(name, entry) {
            Ok(value) => values.push(value),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    if diagnostics.is_empty() {
        Ok(values)
    } else {
        Err(diagnostics)
    }
}

pub(super) fn decode_ui_component(node: &Node) -> Result<Located<String>, Diagnostic> {
    let mapping = expect_mapping(node, "field `ui`")?;
    ensure_known_keys(mapping, &["component"], "field `ui`")?;
    let component = required(mapping, "component", &node.span)?;
    expect_string(&component.value, "field `ui.component`")
}

fn decode_value_object(name: &str, entry: &MappingEntry) -> Result<SurfaceValueObject, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "value object definition")?;
    ensure_known_keys(mapping, &["fields"], "value object definition")?;
    let fields_node = required(mapping, "fields", &entry.value.span)?;
    let fields = expect_mapping(&fields_node.value, "value object `fields`")?
        .iter()
        .map(|(field_name, field)| decode_value_field(field_name, field))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SurfaceValueObject {
        name: located_name(name, entry),
        fields,
        span: entry.value.span.clone(),
    })
}

fn decode_value_field(name: &str, entry: &MappingEntry) -> Result<SurfaceValueField, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "value object field")?;
    ensure_known_keys(
        mapping,
        &["type", "required", "values"],
        "value object field",
    )?;
    let type_node = required(mapping, "type", &entry.value.span)?;
    let values = mapping
        .get("values")
        .map(|values| {
            expect_sequence(&values.value, "enum `values`")?
                .iter()
                .map(|value| expect_string(value, "enum value"))
                .collect()
        })
        .transpose()?;
    Ok(SurfaceValueField {
        name: located_name(name, entry),
        type_name: expect_string(&type_node.value, "value object field `type`")?,
        required: optional_bool(mapping, "required")?,
        values,
        span: entry.value.span.clone(),
    })
}

fn decode_operation(
    name: &str,
    entry: &MappingEntry,
    collection: &str,
) -> Result<SurfaceOperation, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "operation definition")?;
    ensure_known_keys(
        mapping,
        &["input", "output", "access"],
        "operation definition",
    )?;
    let output = required(mapping, "output", &entry.value.span)?;
    let access = required(mapping, "access", &entry.value.span)?;
    let input = optional_string(mapping, "input", "operation `input`")?;
    if collection == "commands" && input.is_none() {
        return Err(Diagnostic::error(
            "AS1007",
            "command requires `input`",
            entry.value.span.clone(),
        ));
    }
    Ok(SurfaceOperation {
        name: located_name(name, entry),
        input,
        output: expect_string(&output.value, "operation `output`")?,
        access: decode_rule(&access.value)?,
        span: entry.value.span.clone(),
    })
}

fn decode_page(name: &str, entry: &MappingEntry) -> Result<SurfacePage, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "page definition")?;
    ensure_known_keys(mapping, &["label", "path", "component"], "page definition")?;
    let path = required(mapping, "path", &entry.value.span)?;
    let component = required(mapping, "component", &entry.value.span)?;
    Ok(SurfacePage {
        name: located_name(name, entry),
        label: optional_string(mapping, "label", "page `label`")?,
        path: expect_string(&path.value, "page `path`")?,
        component: expect_string(&component.value, "page `component`")?,
        span: entry.value.span.clone(),
    })
}

fn located_name(name: &str, entry: &MappingEntry) -> Located<String> {
    Located {
        value: name.to_owned(),
        span: entry.key_span.clone(),
    }
}
