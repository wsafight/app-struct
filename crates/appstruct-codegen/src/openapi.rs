use crate::{Artifact, ArtifactKind, CodegenError, format_rust, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr};
use serde_json::{Map, Value, json};

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    Ok(vec![Artifact::text(
        "openapi/openapi.json",
        document_text(ir)?,
        ArtifactKind::OpenApi,
    )])
}

pub(crate) fn rust_source(ir: &AppIr) -> Result<String, CodegenError> {
    let document = document_text(ir)?;
    format_rust(&format!(
        "{}pub const OPENAPI_JSON: &str = {:?};\n",
        generated_header("//"),
        document
    ))
}

fn document_text(ir: &AppIr) -> Result<String, CodegenError> {
    let mut output = serde_json::to_string_pretty(&document(ir))?;
    output.push('\n');
    Ok(output)
}

fn document(ir: &AppIr) -> Value {
    let mut paths = Map::new();
    let mut schemas = Map::new();
    schemas.insert("Error".to_owned(), error_schema());
    for entity in &ir.entities {
        add_entity_paths(&mut paths, entity);
        add_entity_schemas(&mut schemas, entity);
    }
    if ir.auth.enabled {
        auth::add(&mut paths, &mut schemas, ir);
    }
    if ir.tenant.enabled {
        tenant::add(&mut paths, &mut schemas);
    }
    extension::add(ir, &mut paths, &mut schemas);
    let security_schemes = auth::security_schemes(ir.auth.enabled);
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": ir.app.name,
            "version": "0.1.0",
        },
        "paths": paths,
        "components": { "schemas": schemas, "securitySchemes": security_schemes },
    })
}

fn add_entity_paths(paths: &mut Map<String, Value>, entity: &EntityIr) {
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

fn add_entity_schemas(schemas: &mut Map<String, Value>, entity: &EntityIr) {
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

pub(super) fn schema_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

pub(super) fn request_body(schema: &str) -> Value {
    json!({
        "required": true,
        "content": { "application/json": { "schema": schema_ref(schema) } }
    })
}

pub(super) fn response(description: &str, schema: &Value) -> Value {
    json!({
        "description": description,
        "content": { "application/json": { "schema": schema } }
    })
}

fn versioned_response(description: &str, schema: &Value) -> Value {
    let mut value = response(description, schema);
    value["headers"] = json!({
        "ETag": {
            "description": "Optimistic concurrency revision",
            "schema": { "type": "string", "pattern": "^\\\"rev-[1-9][0-9]*\\\"$" }
        }
    });
    value
}

fn if_match_parameter() -> Value {
    json!({
        "name": "If-Match",
        "in": "header",
        "required": true,
        "schema": { "type": "string", "pattern": "^\\\"rev-[1-9][0-9]*\\\"$" }
    })
}

pub(super) fn error_response() -> Value {
    response("Error", &schema_ref("Error"))
}

fn error_schema() -> Value {
    json!({
        "type": "object",
        "required": ["error"],
        "properties": {
            "error": {
                "type": "object",
                "required": ["code", "message", "fields"],
                "properties": {
                    "code": { "type": "string" },
                    "message": { "type": "string" },
                    "fields": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["field", "message"],
                            "properties": {
                                "field": { "type": "string" },
                                "message": { "type": "string" },
                            }
                        }
                    }
                }
            }
        }
    })
}
mod auth;
mod extension;
mod tenant;
