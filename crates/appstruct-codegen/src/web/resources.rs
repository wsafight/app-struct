use crate::generated_header;
use appstruct_ir::{AccessRuleIr, AppIr, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr};
use std::fmt::Write;

pub(super) fn source(ir: &AppIr) -> String {
    let imports = ir
        .entities
        .iter()
        .map(|entity| {
            format!(
                "import {{ {}Api }} from \"./client\";",
                lower_camel(&entity.rust_name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let resources = ir
        .entities
        .iter()
        .map(resource_source)
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{}import type {{ AccessRule, ResourceApi, ResourceDefinition }} from \"../resource\";\n{}\n\nexport const resources: ResourceDefinition[] = [\n{}\n];\n\nexport const auditAccess: AccessRule = {};\n",
        generated_header("//"),
        imports,
        indent(&resources, 2),
        audit_access_source(ir),
    )
}

fn resource_source(entity: &EntityIr) -> String {
    let fields = entity
        .fields
        .iter()
        .filter(|field| {
            !matches!(
                field.generated,
                Some(GeneratedValueIr::Revision | GeneratedValueIr::Tenant)
            )
        })
        .map(field_source)
        .collect::<Vec<_>>()
        .join(",\n");
    let primary_key = entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .map_or("id", |field| field.rust_name.as_str());
    let api = format!("{}Api", lower_camel(&entity.rust_name));
    format!(
        "{{\n  id: {:?},\n  name: {:?},\n  label: {:?},\n  slug: {:?},\n  primaryKey: {:?},\n  access: {},\n  fields: [\n{}\n  ],\n  api: {} as unknown as ResourceApi,\n}}",
        entity.id.0,
        entity.rust_name,
        entity.label,
        entity.table_name,
        primary_key,
        serde_json::to_string(&entity.access).expect("access IR is serializable"),
        indent(&fields, 4),
        api,
    )
}

fn audit_access_source(ir: &AppIr) -> String {
    let rules = ir
        .audit
        .reader_roles
        .iter()
        .map(|role| AccessRuleIr::Role { role: role.clone() })
        .collect::<Vec<_>>();
    let rule = match rules.as_slice() {
        [rule] => rule.clone(),
        _ => AccessRuleIr::Any { rules },
    };
    serde_json::to_string(&rule).expect("access IR is serializable")
}

fn field_source(field: &FieldIr) -> String {
    let mut properties = vec![
        format!("name: {:?}", field.rust_name),
        format!("label: {:?}", humanize(&field.api_name)),
        format!("kind: {:?}", field_kind(&field.ty)),
        format!("required: {}", !field.nullable && field.default.is_none()),
        format!("readOnly: {}", field.generated.is_some()),
        format!("primaryKey: {}", field.primary_key),
        format!("searchable: {}", field.capabilities.searchable),
        format!("filterable: {}", field.capabilities.filterable),
        format!("sortable: {}", field.capabilities.sortable),
    ];
    if let FieldTypeIr::Enum { values } = &field.ty {
        properties.push(format!("values: {values:?}"));
    }
    if let FieldTypeIr::Relation { target } = &field.ty {
        properties.push(format!("relation: {:?}", target.0));
    }
    if let Some(minimum) = &field.validation.minimum {
        properties.push(format!("minimum: {minimum:?}"));
    }
    if let Some(maximum) = &field.validation.maximum {
        properties.push(format!("maximum: {maximum:?}"));
    }
    if let Some(component) = &field.ui_component {
        properties.push(format!("uiComponent: {component:?}"));
    }
    if let Some(access) = &field.read_access {
        properties.push(format!(
            "readAccess: {}",
            serde_json::to_string(access).expect("access IR is serializable")
        ));
    }
    if let Some(access) = &field.write_access {
        properties.push(format!(
            "writeAccess: {}",
            serde_json::to_string(access).expect("access IR is serializable")
        ));
    }
    format!("{{ {} }}", properties.join(", "))
}

fn field_kind(field_type: &FieldTypeIr) -> &'static str {
    match field_type {
        FieldTypeIr::Uuid => "uuid",
        FieldTypeIr::String => "string",
        FieldTypeIr::Text => "text",
        FieldTypeIr::Integer => "integer",
        FieldTypeIr::Bigint => "bigint",
        FieldTypeIr::Decimal => "decimal",
        FieldTypeIr::Boolean => "boolean",
        FieldTypeIr::Date => "date",
        FieldTypeIr::Datetime => "datetime",
        FieldTypeIr::Json => "json",
        FieldTypeIr::Enum { .. } => "enum",
        FieldTypeIr::Relation { .. } => "relation",
    }
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

fn humanize(value: &str) -> String {
    let value = value.strip_suffix("_id").unwrap_or(value);
    let mut words = value.split('_');
    words.next().map_or_else(String::new, |first| {
        let mut characters = first.chars();
        let first = characters
            .next()
            .map_or_else(String::new, |character| character.to_uppercase().collect());
        let mut output = format!("{first}{}", characters.collect::<String>());
        for word in words {
            let _ = write!(output, " {word}");
        }
        output
    })
}

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
