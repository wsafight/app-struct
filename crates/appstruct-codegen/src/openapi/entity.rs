use super::{
    auth, error_response, if_match_parameter, request_body, response, schema_ref, tenant,
    versioned_response,
};
use appstruct_ir::{EntityIr, FieldIr, FieldTypeIr};
use serde_json::{Map, Value, json};

pub(super) fn add_paths(paths: &mut Map<String, Value>, entity: &EntityIr) {
    let singular = &entity.rust_name;
    let collection = format!("/api/{}/", entity.table_name);
    let member = format!("/api/{}/{{id}}", entity.table_name);
    let mut list_parameters = list_parameters(entity);
    let mut member_parameters = vec![json!({
        "name": "id",
        "in": "path",
        "required": true,
        "schema": primary_key_schema(entity),
    })];
    let create_parameters = if entity.tenant_scoped {
        list_parameters.push(tenant::parameter());
        member_parameters.push(tenant::parameter());
        vec![tenant::parameter()]
    } else {
        Vec::new()
    };
    paths.insert(
        collection,
        json!({
            "get": {
                "operationId": format!("list{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.list),
                "parameters": list_parameters,
                "responses": {
                    "200": response(
                        "Paginated resource collection",
                        &schema_ref(&format!("{singular}ListResponse"))
                    )
                }
            },
            "post": {
                "operationId": format!("create{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.create),
                "parameters": create_parameters,
                "requestBody": request_body(&format!("Create{singular}Input")),
                "responses": {
                    "201": versioned_response("Resource created", &schema_ref(singular)),
                    "422": error_response(),
                }
            }
        }),
    );
    paths.insert(
        member,
        json!({
            "parameters": member_parameters,
            "get": {
                "operationId": format!("get{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.read),
                "responses": {
                    "200": versioned_response("Resource", &schema_ref(singular)),
                    "404": error_response(),
                }
            },
            "patch": {
                "operationId": format!("update{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.update),
                "parameters": [if_match_parameter()],
                "requestBody": request_body(&format!("Update{singular}Input")),
                "responses": {
                    "200": versioned_response("Resource updated", &schema_ref(singular)),
                    "404": error_response(),
                    "412": error_response(),
                    "422": error_response(),
                    "428": error_response(),
                }
            },
            "delete": {
                "operationId": format!("delete{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.delete),
                "parameters": [if_match_parameter()],
                "responses": {
                    "204": { "description": "Resource deleted" },
                    "404": error_response(),
                    "412": error_response(),
                    "428": error_response(),
                }
            }
        }),
    );
}

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
}

fn entity_schema(entity: &EntityIr) -> Value {
    let properties = entity
        .fields
        .iter()
        .map(|field| (field.rust_name.clone(), field_schema(field, true)))
        .collect::<Map<_, _>>();
    let required = entity
        .fields
        .iter()
        .filter(|field| !field.nullable)
        .map(|field| Value::String(field.rust_name.clone()))
        .collect::<Vec<_>>();
    json!({ "type": "object", "properties": properties, "required": required })
}

fn input_schema(entity: &EntityIr, update: bool) -> Value {
    let fields = entity.fields.iter().filter(|field| {
        if update {
            !field.primary_key && field.generated.is_none()
        } else {
            field.generated.is_none()
        }
    });
    let fields = fields.collect::<Vec<_>>();
    let properties = fields
        .iter()
        .map(|field| (field.rust_name.clone(), field_schema(field, false)))
        .collect::<Map<_, _>>();
    let required = if update {
        Vec::new()
    } else {
        fields
            .iter()
            .filter(|field| !field.nullable && field.default.is_none())
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
                "required": ["page", "page_size", "total"],
                "properties": {
                    "page": { "type": "integer", "minimum": 1 },
                    "page_size": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "total": { "type": "integer", "minimum": 0 },
                }
            }
        }
    })
}

fn list_parameters(entity: &EntityIr) -> Vec<Value> {
    let mut parameters = vec![
        query_parameter(
            "page",
            &json!({ "type": "integer", "minimum": 1, "default": 1 }),
        ),
        query_parameter(
            "page_size",
            &json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }),
        ),
        query_parameter("sort", &json!({ "type": "string" })),
        query_parameter("q", &json!({ "type": "string" })),
    ];
    for field in entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
    {
        parameters.push(query_parameter(
            &format!("filter[{}]", field.rust_name),
            &field_schema(field, false),
        ));
        if matches!(
            field.ty,
            FieldTypeIr::Integer
                | FieldTypeIr::Bigint
                | FieldTypeIr::Decimal
                | FieldTypeIr::Date
                | FieldTypeIr::Datetime
        ) {
            for operator in ["gte", "lte"] {
                parameters.push(query_parameter(
                    &format!("filter[{}][{operator}]", field.rust_name),
                    &field_schema(field, false),
                ));
            }
        }
    }
    parameters
}

fn query_parameter(name: &str, schema: &Value) -> Value {
    json!({ "name": name, "in": "query", "required": false, "schema": schema })
}

fn field_schema(field: &FieldIr, response: bool) -> Value {
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
    if field.nullable {
        schema["type"] = match schema.get("type").cloned() {
            Some(Value::String(value)) => json!([value, "null"]),
            _ => json!(["object", "array", "string", "number", "boolean", "null"]),
        };
    }
    if response && field.generated.is_some() {
        schema["readOnly"] = Value::Bool(true);
    }
    if let Some(minimum) = field.validation.min_length {
        schema["minLength"] = json!(minimum);
    }
    if let Some(maximum) = field.validation.max_length {
        schema["maxLength"] = json!(maximum);
    }
    if let Some(minimum) = &field.validation.minimum {
        schema["minimum"] = json_number(minimum);
    }
    if let Some(maximum) = &field.validation.maximum {
        schema["maximum"] = json_number(maximum);
    }
    schema
}

fn json_number(value: &str) -> Value {
    value.parse::<i64>().map_or_else(
        |_| json!(value.parse::<f64>().unwrap_or_default()),
        Value::from,
    )
}

fn primary_key_schema(entity: &EntityIr) -> Value {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .map_or_else(
            || json!({ "type": "string" }),
            |field| field_schema(field, false),
        )
}
