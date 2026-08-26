use super::{error_response, request_body, response, schema_ref};
use appstruct_ir::{AppIr, FieldTypeIr, OperationTypeIr, ValueFieldIr};
use serde_json::{Map, Value, json};

pub(super) fn add(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    for value in &ir.value_objects {
        let properties = value
            .fields
            .iter()
            .map(|field| (field.rust_name.clone(), value_field_schema(field)))
            .collect::<Map<_, _>>();
        let required = value
            .fields
            .iter()
            .filter(|field| field.required)
            .map(|field| field.rust_name.clone())
            .collect::<Vec<_>>();
        schemas.insert(
            value.rust_name.clone(),
            json!({ "type": "object", "properties": properties, "required": required }),
        );
    }
    for command in &ir.commands {
        paths.insert(
            format!("/api/commands/{}", kebab_name(&command.rust_name)),
            json!({
                "post": {
                    "operationId": lower_camel(&command.rust_name),
                    "tags": ["Commands"],
                    "security": super::auth::security(&command.access),
                    "requestBody": request_body(&type_name(ir, &command.input)),
                    "responses": {
                        "200": response("Command result", &type_schema(ir, &command.output)),
                        "403": error_response(),
                        "422": error_response(),
                    }
                }
            }),
        );
    }
    for query in &ir.queries {
        let method = if let Some(input) = &query.input {
            json!({
                "operationId": lower_camel(&query.rust_name),
                "tags": ["Queries"],
                "security": super::auth::security(&query.access),
                "requestBody": request_body(&type_name(ir, input)),
                "responses": {
                    "200": response("Query result", &type_schema(ir, &query.output)),
                    "403": error_response(),
                }
            })
        } else {
            json!({
                "operationId": lower_camel(&query.rust_name),
                "tags": ["Queries"],
                "security": super::auth::security(&query.access),
                "responses": {
                    "200": response("Query result", &type_schema(ir, &query.output)),
                    "403": error_response(),
                }
            })
        };
        let verb = if query.input.is_some() { "post" } else { "get" };
        paths.insert(
            format!("/api/queries/{}", kebab_name(&query.rust_name)),
            Map::from_iter([(verb.to_owned(), method)]).into(),
        );
    }
}

fn value_field_schema(field: &ValueFieldIr) -> Value {
    let mut schema = match &field.ty {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => {
            json!({ "type": "string", "format": "uuid" })
        }
        FieldTypeIr::String | FieldTypeIr::Text => json!({ "type": "string" }),
        FieldTypeIr::Integer => json!({ "type": "integer", "format": "int32" }),
        FieldTypeIr::Bigint => json!({ "type": "integer", "format": "int64" }),
        FieldTypeIr::Decimal => json!({ "type": "string", "format": "decimal" }),
        FieldTypeIr::Boolean => json!({ "type": "boolean" }),
        FieldTypeIr::Date => json!({ "type": "string", "format": "date" }),
        FieldTypeIr::Datetime => json!({ "type": "string", "format": "date-time" }),
        FieldTypeIr::Json => json!({}),
        FieldTypeIr::Enum { values } => json!({ "type": "string", "enum": values }),
    };
    if !field.required {
        schema["nullable"] = Value::Bool(true);
    }
    schema
}

fn type_schema(ir: &AppIr, operation_type: &OperationTypeIr) -> Value {
    schema_ref(&type_name(ir, operation_type))
}

fn type_name(ir: &AppIr, operation_type: &OperationTypeIr) -> String {
    match operation_type {
        OperationTypeIr::Entity { entity } => ir
            .entities
            .iter()
            .find(|candidate| candidate.id == *entity)
            .map(|entity| entity.rust_name.clone())
            .expect("compiler resolved operation entity"),
        OperationTypeIr::ValueObject { value_object } => ir
            .value_objects
            .iter()
            .find(|candidate| candidate.id == *value_object)
            .map(|value| value.rust_name.clone())
            .expect("compiler resolved operation value object"),
    }
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

fn kebab_name(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('-');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}
