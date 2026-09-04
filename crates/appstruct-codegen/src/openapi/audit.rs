use super::{error_response, response, schema_ref};
use appstruct_ir::AppIr;
use serde_json::{Map, Value, json};

pub(super) fn add(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("AuditEvent".to_owned(), event_schema());
    schemas.insert("AuditEventListResponse".to_owned(), list_schema());
    let mut parameters = vec![
        query_parameter("page", 1, None),
        query_parameter("page_size", 1, Some(100)),
    ];
    if ir.tenant.enabled {
        parameters.push(super::tenant::parameter());
    }
    paths.insert(
        "/api/audit/events".to_owned(),
        json!({
            "get": {
                "operationId": "listAuditEvents",
                "tags": ["Audit"],
                "security": [{ "cookieSession": [] }],
                "parameters": parameters,
                "responses": {
                    "200": response("Audit events", &schema_ref("AuditEventListResponse")),
                    "400": error_response(),
                    "401": error_response(),
                    "403": error_response()
                }
            }
        }),
    );
}

fn query_parameter(name: &str, minimum: u64, maximum: Option<u64>) -> Value {
    let mut schema = json!({ "type": "integer", "minimum": minimum });
    if let Some(maximum) = maximum {
        schema["maximum"] = json!(maximum);
    }
    json!({ "name": name, "in": "query", "required": false, "schema": schema })
}

fn event_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "entity", "record_id", "operation", "actor_id", "tenant_id", "before", "after", "occurred_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "entity": { "type": "string" },
            "record_id": { "type": "string" },
            "operation": { "type": "string", "enum": ["create", "update", "delete", "restore"] },
            "actor_id": { "type": ["string", "null"], "format": "uuid" },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "before": { "type": ["object", "array", "string", "number", "boolean", "null"] },
            "after": { "type": ["object", "array", "string", "number", "boolean", "null"] },
            "occurred_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn list_schema() -> Value {
    json!({
        "type": "object",
        "required": ["data", "meta"],
        "properties": {
            "data": { "type": "array", "items": schema_ref("AuditEvent") },
            "meta": {
                "type": "object",
                "required": ["page", "page_size", "total"],
                "properties": {
                    "page": { "type": "integer" },
                    "page_size": { "type": "integer" },
                    "total": { "type": "integer" }
                }
            }
        }
    })
}
