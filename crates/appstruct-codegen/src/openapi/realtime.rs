use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("PresenceEntry".to_owned(), presence_schema());
    paths.insert("/api/realtime/events".to_owned(), events_path());
    paths.insert("/api/realtime/presence".to_owned(), presence_path());
}

fn scope_parameters() -> Value {
    json!([
        { "name": "tenant_id", "in": "query", "schema": { "type": "string", "format": "uuid" } },
        { "name": "resource", "in": "query", "schema": { "type": "string", "maxLength": 120 } },
        { "name": "record_id", "in": "query", "schema": { "type": "string", "maxLength": 200 } }
    ])
}

fn events_path() -> Value {
    json!({
        "get": {
            "operationId": "subscribeRealtime", "tags": ["Realtime"],
            "security": [{ "cookieSession": [] }],
            "parameters": scope_parameters(),
            "responses": {
                "200": {
                    "description": "Server-sent event stream",
                    "content": { "text/event-stream": { "schema": { "type": "string" } } }
                },
                "400": error_response(), "401": error_response(), "403": error_response()
            }
        }
    })
}

fn presence_path() -> Value {
    json!({
        "get": {
            "operationId": "listPresence", "tags": ["Realtime"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": scope_parameters(),
            "responses": {
                "200": response("Online presence", &json!({
                    "type": "object", "required": ["data"],
                    "properties": { "data": { "type": "array", "items": schema_ref("PresenceEntry") } }
                })),
                "400": error_response(), "401": error_response(), "403": error_response()
            }
        }
    })
}

fn presence_schema() -> Value {
    json!({
        "type": "object",
        "required": ["connection_id", "actor_id", "tenant_id", "resource", "record_id", "connected_at", "last_seen_at", "expires_at"],
        "properties": {
            "connection_id": { "type": "string", "format": "uuid" },
            "actor_id": { "type": "string", "format": "uuid" },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "resource": { "type": ["string", "null"] },
            "record_id": { "type": ["string", "null"] },
            "connected_at": { "type": "string", "format": "date-time" },
            "last_seen_at": { "type": "string", "format": "date-time" },
            "expires_at": { "type": "string", "format": "date-time" }
        }
    })
}
