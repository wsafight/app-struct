use super::super::schema_ref;
use appstruct_ir::{EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr};
use serde_json::{Map, Value, json};

pub(super) fn add_schemas(schemas: &mut Map<String, Value>, entity: &EntityIr) {
    schemas.insert(entity.rust_name.clone(), entity_schema(entity));
    schemas.insert(
        format!("{}ListResponse", entity.rust_name),
        list_response_schema(entity),
    );
    schemas.insert(
        format!("Create{}Input", entity.rust_name),
        input_schema(entity, false),
    );
    schemas.insert(
        format!("Update{}Input", entity.rust_name),
        input_schema(entity, true),
    );
    schemas.insert(
        format!("{}AggregateResponse", entity.rust_name),
        aggregate_response_schema(),
    );
    super::super::bulk::add_schemas(schemas, entity);
    add_workflow_schemas(schemas, entity);
}

fn add_workflow_schemas(schemas: &mut Map<String, Value>, entity: &EntityIr) {
    let Some(workflow) = &entity.workflow else {
        return;
    };
    let singular = &entity.rust_name;
    schemas.insert(
        format!("{singular}WorkflowTransitionCapability"),
        json!({
            "type": "object",
            "required": ["name", "to", "input"],
            "properties": {
                "name": {
                    "type": "string",
                    "enum": workflow.transitions.iter().map(|transition| &transition.name).collect::<Vec<_>>(),
                },
                "to": { "type": "string" },
                "input": { "type": ["string", "null"] },
            },
        }),
    );
    schemas.insert(
        format!("{singular}WorkflowCapabilities"),
        json!({
            "type": "object",
            "required": ["state", "revision", "allowed_transitions"],
            "properties": {
                "state": { "type": "string" },
                "revision": { "type": "integer", "format": "int64", "minimum": 1 },
                "allowed_transitions": {
                    "type": "array",
                    "items": schema_ref(&format!("{singular}WorkflowTransitionCapability")),
                },
            },
        }),
    );
    let mut inputs = workflow
        .transitions
        .iter()
        .filter_map(|transition| transition.input.as_ref())
        .map(|input| schema_ref(input.trim_start_matches("app::")))
        .collect::<Vec<_>>();
    if workflow
        .transitions
        .iter()
        .any(|transition| transition.input.is_none())
    {
        inputs.push(json!({ "type": "object", "maxProperties": 0 }));
    }
    schemas.insert(
        format!("{singular}WorkflowTransitionInput"),
        json!({ "oneOf": inputs }),
    );
}

fn entity_schema(entity: &EntityIr) -> Value {
    let properties = entity
        .fields
        .iter()
        .map(|field| {
            let mut schema = field_schema(field, true);
            if entity.is_workflow_field(field) {
                schema["readOnly"] = Value::Bool(true);
            }
            (field.rust_name.clone(), schema)
        })
        .collect::<Map<_, _>>();
    let required = entity
        .fields
        .iter()
        .filter(|field| !field.nullable && field.read_access.is_none())
        .map(|field| Value::String(field.rust_name.clone()))
        .collect::<Vec<_>>();
    json!({ "type": "object", "properties": properties, "required": required })
}

pub(super) fn input_schema(entity: &EntityIr, update: bool) -> Value {
    let fields = entity
        .fields
        .iter()
        .filter(|field| {
            if update {
                !field.primary_key && field.generated.is_none() && !entity.is_workflow_field(field)
            } else {
                field.generated.is_none() && !entity.is_workflow_field(field)
            }
        })
        .collect::<Vec<_>>();
    let properties = fields
        .iter()
        .map(|field| (field.rust_name.clone(), field_schema(field, false)))
        .collect::<Map<_, _>>();
    let required = if update {
        Vec::new()
    } else {
        fields
            .iter()
            .filter(|field| {
                !field.nullable && field.default.is_none() && field.write_access.is_none()
            })
            .map(|field| Value::String(field.rust_name.clone()))
            .collect()
    };
    json!({ "type": "object", "properties": properties, "required": required })
}

fn list_response_schema(entity: &EntityIr) -> Value {
    json!({
        "type": "object",
        "required": ["data", "meta"],
        "properties": {
            "data": { "type": "array", "items": schema_ref(&entity.rust_name) },
            "meta": {
                "type": "object",
                "oneOf": [
                    {
                        "title": "Offset pagination",
                        "required": ["page", "page_size", "total"],
                        "properties": {
                            "page": { "type": "integer", "minimum": 1 },
                            "page_size": { "type": "integer", "minimum": 1, "maximum": 100 },
                            "total": { "type": "integer", "minimum": 0 },
                        }
                    },
                    {
                        "title": "Cursor pagination",
                        "required": ["limit", "next_cursor", "has_more"],
                        "properties": {
                            "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                            "next_cursor": { "type": ["string", "null"] },
                            "has_more": { "type": "boolean" },
                        }
                    }
                ]
            }
        }
    })
}

fn aggregate_response_schema() -> Value {
    json!({
        "type": "object",
        "required": ["data", "meta"],
        "properties": {
            "data": {
                "type": "array",
                "items": { "type": "object", "additionalProperties": true }
            },
            "meta": {
                "type": "object",
                "required": ["metrics", "group_by", "limit"],
                "properties": {
                    "metrics": { "type": "array", "items": { "type": "string" } },
                    "group_by": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                }
            }
        }
    })
}

pub(super) fn field_schema(field: &FieldIr, response: bool) -> Value {
    let mut schema = match &field.ty {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => {
            json!({ "type": "string", "format": "uuid" })
        }
        FieldTypeIr::String | FieldTypeIr::Text => json!({ "type": "string" }),
        FieldTypeIr::Integer => json!({ "type": "integer", "format": "int32" }),
        FieldTypeIr::Bigint => {
            json!({ "type": "string", "format": "int64", "pattern": "^-?[0-9]+$", "description": "Signed 64-bit integer encoded as a decimal string." })
        }
        FieldTypeIr::Decimal => json!({ "type": "string", "format": "decimal" }),
        FieldTypeIr::Boolean => json!({ "type": "boolean" }),
        FieldTypeIr::Date => json!({ "type": "string", "format": "date" }),
        FieldTypeIr::Datetime => json!({ "type": "string", "format": "date-time" }),
        FieldTypeIr::Json => json!({}),
        FieldTypeIr::Enum { values } => json!({ "type": "string", "enum": values }),
    };
    if field.nullable {
        schema["type"] = match schema.get("type").cloned() {
            Some(Value::String(value)) => json!([value, "null"]),
            _ => json!(["object", "array", "string", "number", "boolean", "null"]),
        };
    }
    if response && field.generated.is_some() {
        schema["readOnly"] = Value::Bool(true);
    }
    if matches!(field.generated, Some(GeneratedValueIr::Revision)) {
        schema = json!({ "type": "integer", "format": "int64", "readOnly": true });
    }
    if let Some(access) = &field.read_access {
        schema["x-appstruct-read-access"] =
            serde_json::to_value(access).expect("access is serializable");
    }
    if let Some(access) = &field.write_access {
        schema["x-appstruct-write-access"] =
            serde_json::to_value(access).expect("access is serializable");
    }
    if let Some(minimum) = field.validation.min_length {
        schema["minLength"] = json!(minimum);
    }
    if let Some(maximum) = field.validation.max_length {
        schema["maxLength"] = json!(maximum);
    }
    if let Some(minimum) = &field.validation.minimum {
        if matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal) {
            schema["x-appstruct-minimum"] = json!(minimum);
        } else {
            schema["minimum"] = serde_json::from_str(minimum).unwrap_or_else(|_| json!(0));
        }
    }
    if let Some(maximum) = &field.validation.maximum {
        if matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal) {
            schema["x-appstruct-maximum"] = json!(maximum);
        } else {
            schema["maximum"] = serde_json::from_str(maximum).unwrap_or_else(|_| json!(0));
        }
    }
    schema
}

pub(super) fn primary_key(entity: &EntityIr) -> Value {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .map_or_else(
            || json!({ "type": "string" }),
            |field| field_schema(field, false),
        )
}
