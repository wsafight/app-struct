use super::{
    auth, error_response, if_match_parameter, request_body, response, schema_ref, tenant,
    versioned_response,
};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr};
use serde_json::{Map, Value, json};
pub(super) fn add_paths(paths: &mut Map<String, Value>, ir: &AppIr, entity: &EntityIr) {
    let singular = &entity.rust_name;
    let collection = format!("/api/{}/", entity.table_name);
    let member = format!("/api/{}/{{id}}", entity.table_name);
    let mut list_parameters = list_parameters(ir, entity);
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
    add_aggregate_path(paths, ir, entity);
    super::bulk::add_paths(paths, ir, entity);
}
fn add_aggregate_path(paths: &mut Map<String, Value>, ir: &AppIr, entity: &EntityIr) {
    let singular = &entity.rust_name;
    let aggregate_path = format!("/api/{}/_aggregate", entity.table_name);
    paths.insert(
        aggregate_path,
        json!({
            "get": {
                "operationId": format!("aggregate{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.list),
                "parameters": aggregate_parameters(ir, entity),
                "responses": {
                    "200": response(
                        "Aggregate and grouped resource data",
                        &schema_ref(&format!("{singular}AggregateResponse"))
                    ),
                    "400": error_response(),
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
    schemas.insert(
        format!("{}AggregateResponse", entity.rust_name),
        aggregate_response_schema(),
    );
    super::bulk::add_schemas(schemas, entity);
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
        .filter(|field| !field.nullable && field.read_access.is_none())
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
fn list_parameters(ir: &AppIr, entity: &EntityIr) -> Vec<Value> {
    let mut parameters = vec![
        query_parameter(
            "page",
            &json!({ "type": "integer", "minimum": 1, "default": 1 }),
        ),
        query_parameter(
            "page_size",
            &json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }),
        ),
        query_parameter("cursor", &json!({ "type": "string" })),
        query_parameter(
            "limit",
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
    for relation_field in entity.fields.iter().filter(|field| {
        field.capabilities.filterable && matches!(field.ty, FieldTypeIr::Relation { .. })
    }) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        let target = ir
            .entities
            .iter()
            .find(|candidate| candidate.id == *target)
            .expect("validated relation target");
        for target_field in target
            .fields
            .iter()
            .filter(|field| field.capabilities.filterable)
        {
            parameters.push(query_parameter(
                &format!(
                    "filter[{}.{}]",
                    relation_field.api_name, target_field.rust_name
                ),
                &field_schema(target_field, false),
            ));
            if matches!(
                target_field.ty,
                FieldTypeIr::Integer
                    | FieldTypeIr::Bigint
                    | FieldTypeIr::Decimal
                    | FieldTypeIr::Date
                    | FieldTypeIr::Datetime
            ) {
                for operator in ["gte", "lte"] {
                    parameters.push(query_parameter(
                        &format!(
                            "filter[{}.{}][{operator}]",
                            relation_field.api_name, target_field.rust_name
                        ),
                        &field_schema(target_field, false),
                    ));
                }
            }
        }
    }
    parameters
}
fn aggregate_parameters(ir: &AppIr, entity: &EntityIr) -> Vec<Value> {
    let mut parameters = vec![
        query_parameter(
            "metrics",
            &json!({ "type": "string", "example": "count,sum:priority,avg:priority" }),
        ),
        query_parameter(
            "group_by",
            &json!({ "type": "string", "example": "status" }),
        ),
        query_parameter(
            "limit",
            &json!({ "type": "integer", "minimum": 1, "maximum": 500, "default": 100 }),
        ),
    ];
    parameters.extend(list_parameters(ir, entity).into_iter().filter(|parameter| {
        let name = parameter["name"].as_str().unwrap_or_default();
        name == "q" || name.starts_with("filter[")
    }));
    parameters
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
        schema["minimum"] = serde_json::from_str(minimum).unwrap_or_else(|_| json!(0));
    }
    if let Some(maximum) = &field.validation.maximum {
        schema["maximum"] = serde_json::from_str(maximum).unwrap_or_else(|_| json!(0));
    }
    schema
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
