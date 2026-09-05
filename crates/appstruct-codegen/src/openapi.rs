use crate::{Artifact, ArtifactKind, CodegenError, format_rust, generated_header};
use appstruct_ir::AppIr;
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
    bulk::add_common_schemas(&mut schemas);
    for entity in &ir.entities {
        entity::add_paths(&mut paths, ir, entity);
        entity::add_schemas(&mut schemas, entity);
    }
    if ir.auth.enabled {
        auth::add(&mut paths, &mut schemas, ir);
    }
    if ir.tenant.enabled {
        tenant::add(&mut paths, &mut schemas);
    }
    if ir.audit.enabled {
        audit::add(ir, &mut paths, &mut schemas);
    }
    if ir.realtime.enabled {
        realtime::add(&mut paths, &mut schemas);
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
mod admin;
mod admin_schedules;
mod admin_storage;
mod audit;
mod auth;
mod bulk;
mod entity;
mod extension;
mod realtime;
mod saved_views;
mod tenant;
