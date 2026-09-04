use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(
    paths: &mut Map<String, Value>,
    schemas: &mut Map<String, Value>,
    jobs_enabled: bool,
    webhooks_enabled: bool,
) {
    schemas.insert("AdminOverview".to_owned(), overview_schema());
    paths.insert("/api/admin/overview".to_owned(), overview_path());
    schemas.insert("AdminUser".to_owned(), user_schema());
    paths.insert("/api/admin/users".to_owned(), users_path());
    schemas.insert(
        "AdminSessionRevocation".to_owned(),
        session_revocation_schema(),
    );
    paths.insert(
        "/api/admin/users/{id}/revoke-sessions".to_owned(),
        revoke_sessions_path(),
    );
    if jobs_enabled {
        add_jobs(paths, schemas);
    }
    if webhooks_enabled {
        add_webhooks(paths, schemas);
    }
}

fn user_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "email", "roles", "email_verified", "active_sessions", "created_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "email": { "type": "string", "format": "email" },
            "roles": { "type": "array", "items": { "type": "string" } },
            "email_verified": { "type": "boolean" },
            "active_sessions": { "type": "integer" },
            "created_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn users_path() -> Value {
    json!({
        "get": {
            "operationId": "listAdminUsers", "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [
                page_parameter(), page_size_parameter()
            ],
            "responses": {
                "200": response("Registered users", &admin_list_schema("AdminUser")),
                "400": error_response(), "401": error_response(), "403": error_response()
            }
        }
    })
}

fn session_revocation_schema() -> Value {
    json!({
        "type": "object",
        "required": ["revoked"],
        "properties": { "revoked": { "type": "integer", "minimum": 0 } }
    })
}

fn revoke_sessions_path() -> Value {
    json!({
        "post": {
            "operationId": "revokeAdminUserSessions", "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter(), { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
            "responses": {
                "200": response("User sessions revoked", &schema_ref("AdminSessionRevocation")),
                "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
            }
        }
    })
}

fn overview_schema() -> Value {
    json!({
        "type": "object",
        "required": ["users", "organizations", "invitations", "sessions", "jobs_queued", "jobs_dead", "mail_deliveries", "files", "audit_events"],
        "properties": {
            "users": { "type": "integer" }, "organizations": { "type": "integer" },
            "invitations": { "type": "integer" }, "sessions": { "type": "integer" },
            "jobs_queued": { "type": "integer" }, "jobs_dead": { "type": "integer" },
            "mail_deliveries": { "type": "integer" }, "files": { "type": "integer" },
            "audit_events": { "type": "integer" }
        }
    })
}

fn overview_path() -> Value {
    json!({
        "get": {
            "operationId": "adminOverview", "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "responses": { "200": response("Operational overview", &schema_ref("AdminOverview")), "401": error_response(), "403": error_response() }
        }
    })
}

fn add_jobs(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("AdminJob".to_owned(), job_schema());
    paths.insert("/api/admin/jobs".to_owned(), jobs_path());
    for (path, operation_id, description, status) in [
        (
            "/api/admin/jobs/{id}/retry",
            "retryAdminJob",
            "Dead job queued for retry",
            "200",
        ),
        (
            "/api/admin/jobs/{id}/replay",
            "replayAdminJob",
            "Terminal job copied into a new queued job",
            "201",
        ),
    ] {
        paths.insert(
            path.to_owned(),
            mutation_path(operation_id, description, status),
        );
    }
}

fn job_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "queue", "kind", "status", "tenant_id", "attempts", "max_attempts", "run_at", "last_error", "created_at", "completed_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "queue": { "type": "string" }, "kind": { "type": "string" },
            "status": { "type": "string", "enum": ["queued", "running", "succeeded", "dead"] },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "attempts": { "type": "integer" }, "max_attempts": { "type": "integer" },
            "run_at": { "type": "string", "format": "date-time" },
            "last_error": { "type": ["string", "null"] },
            "created_at": { "type": "string", "format": "date-time" },
            "completed_at": { "type": ["string", "null"], "format": "date-time" }
        }
    })
}

fn jobs_path() -> Value {
    json!({
        "get": {
            "operationId": "listAdminJobs", "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [
                { "name": "status", "in": "query", "schema": { "type": "string", "enum": ["queued", "running", "succeeded", "dead"] } },
                page_parameter(), page_size_parameter()
            ],
            "responses": {
                "200": response("Recent jobs", &admin_list_schema("AdminJob")),
                "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
            }
        }
    })
}

fn mutation_path(operation_id: &str, description: &str, status: &str) -> Value {
    json!({
        "post": {
            "operationId": operation_id, "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter(), { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
            "responses": {
                (status): response(description, &schema_ref("AdminJob")),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response(), "409": error_response()
            }
        }
    })
}

fn csrf_parameter() -> Value {
    json!({
        "name": "X-CSRF-Token", "in": "header", "required": true,
        "schema": { "type": "string" }
    })
}

fn add_webhooks(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("AdminWebhookDelivery".to_owned(), webhook_schema());
    paths.insert("/api/admin/webhooks".to_owned(), webhooks_path());
    for (path, operation_id, description, status) in [
        (
            "/api/admin/webhooks/{id}/retry",
            "retryAdminWebhook",
            "Dead delivery queued for retry",
            "200",
        ),
        (
            "/api/admin/webhooks/{id}/replay",
            "replayAdminWebhook",
            "Terminal delivery copied into a new pending delivery",
            "201",
        ),
    ] {
        paths.insert(
            path.to_owned(),
            webhook_mutation_path(operation_id, description, status),
        );
    }
}

fn webhook_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "endpoint", "event", "status", "tenant_id", "attempts", "max_attempts", "next_attempt_at", "response_status", "last_error", "created_at", "completed_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "endpoint": { "type": "string" }, "event": { "type": "string" },
            "status": { "type": "string", "enum": ["pending", "delivering", "succeeded", "dead"] },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "attempts": { "type": "integer" }, "max_attempts": { "type": "integer" },
            "next_attempt_at": { "type": "string", "format": "date-time" },
            "response_status": { "type": ["integer", "null"] },
            "last_error": { "type": ["string", "null"] },
            "created_at": { "type": "string", "format": "date-time" },
            "completed_at": { "type": ["string", "null"], "format": "date-time" }
        }
    })
}

fn webhooks_path() -> Value {
    json!({
        "get": {
            "operationId": "listAdminWebhooks", "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [
                { "name": "status", "in": "query", "schema": { "type": "string", "enum": ["pending", "delivering", "succeeded", "dead"] } },
                page_parameter(), page_size_parameter()
            ],
            "responses": {
                "200": response("Recent webhook deliveries", &admin_list_schema("AdminWebhookDelivery")),
                "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
            }
        }
    })
}

fn page_parameter() -> Value {
    json!({ "name": "page", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 1 } })
}

fn page_size_parameter() -> Value {
    json!({ "name": "page_size", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 } })
}

fn admin_list_schema(item: &str) -> Value {
    json!({
        "type": "object",
        "required": ["data", "meta"],
        "properties": {
            "data": { "type": "array", "items": schema_ref(item) },
            "meta": {
                "type": "object",
                "required": ["page", "page_size", "total"],
                "properties": {
                    "page": { "type": "integer", "minimum": 1 },
                    "page_size": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "total": { "type": "integer", "minimum": 0 }
                }
            }
        }
    })
}

fn webhook_mutation_path(operation_id: &str, description: &str, status: &str) -> Value {
    json!({
        "post": {
            "operationId": operation_id, "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter(), { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
            "responses": {
                (status): response(description, &schema_ref("AdminWebhookDelivery")),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response(), "409": error_response()
            }
        }
    })
}
