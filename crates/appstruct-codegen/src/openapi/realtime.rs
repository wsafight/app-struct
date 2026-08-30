use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("PresenceEntry".to_owned(), presence_schema());
    schemas.insert("RealtimeLockLease".to_owned(), lock_schema());
    paths.insert("/api/realtime/events".to_owned(), events_path());
    paths.insert("/api/realtime/presence".to_owned(), presence_path());
    paths.insert("/api/realtime/locks".to_owned(), locks_path());
    paths.insert("/api/realtime/locks/{token}".to_owned(), lock_lease_path());
}

fn lock_scope_parameters() -> Value {
    json!([
        { "name": "tenant_id", "in": "query", "schema": { "type": "string", "format": "uuid" } },
        { "name": "resource", "in": "query", "required": true, "schema": { "type": "string", "maxLength": 120 } },
        { "name": "record_id", "in": "query", "required": true, "schema": { "type": "string", "maxLength": 200 } }
    ])
}

fn scope_parameters() -> Value {
    json!([
        { "name": "tenant_id", "in": "query", "schema": { "type": "string", "format": "uuid" } },
        { "name": "resource", "in": "query", "required": true, "schema": { "type": "string", "maxLength": 120 } },
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

fn locks_path() -> Value {
    let request = json!({
        "required": false,
        "content": { "application/json": { "schema": {
            "type": "object", "properties": {
                "ttl_seconds": { "type": "integer", "minimum": 5, "maximum": 300, "default": 30 }
            }
        } } }
    });
    json!({
        "get": {
            "operationId": "getRealtimeLock", "tags": ["Realtime"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": lock_scope_parameters(),
            "responses": {
                "200": response("Current edit lease", &json!({
                    "type": "object", "required": ["data"],
                    "properties": { "data": { "oneOf": [schema_ref("RealtimeLockLease"), { "type": "null" }] } }
                })),
                "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
            }
        },
        "post": {
            "operationId": "acquireRealtimeLock", "tags": ["Realtime"],
            "security": [{ "cookieSession": [] }], "parameters": lock_scope_parameters(),
            "requestBody": request,
            "responses": {
                "201": response("Edit lease acquired", &schema_ref("RealtimeLockLease")),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response(), "409": error_response()
            }
        }
    })
}

fn lock_lease_path() -> Value {
    let mut parameters = lock_scope_parameters()
        .as_array()
        .cloned()
        .unwrap_or_default();
    parameters.push(json!({
        "name": "token", "in": "path", "required": true,
        "schema": { "type": "string", "format": "uuid" }
    }));
    json!({
        "patch": {
            "operationId": "renewRealtimeLock", "tags": ["Realtime"],
            "security": [{ "cookieSession": [] }], "parameters": parameters,
            "requestBody": {
                "required": false,
                "content": { "application/json": { "schema": {
                    "type": "object", "properties": {
                        "ttl_seconds": { "type": "integer", "minimum": 5, "maximum": 300, "default": 30 }
                    }
                } } }
            },
            "responses": {
                "200": response("Edit lease renewed", &schema_ref("RealtimeLockLease")),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response(), "409": error_response()
            }
        },
        "delete": {
            "operationId": "releaseRealtimeLock", "tags": ["Realtime"],
            "security": [{ "cookieSession": [] }], "parameters": parameters,
            "responses": {
                "204": { "description": "Edit lease released" },
                "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
            }
        }
    })
}

fn lock_schema() -> Value {
    json!({
        "type": "object",
        "required": ["lease_token", "actor_id", "tenant_id", "resource", "record_id", "acquired_at", "expires_at"],
        "properties": {
            "lease_token": { "type": "string", "format": "uuid" },
            "actor_id": { "type": "string", "format": "uuid" },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "resource": { "type": "string" },
            "record_id": { "type": "string" },
            "acquired_at": { "type": "string", "format": "date-time" },
            "expires_at": { "type": "string", "format": "date-time" }
        }
    })
}
