use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(
    paths: &mut Map<String, Value>,
    schemas: &mut Map<String, Value>,
    jobs_enabled: bool,
) {
    schemas.insert("AdminOverview".to_owned(), overview_schema());
    paths.insert("/api/admin/overview".to_owned(), overview_path());
    if jobs_enabled {
        add_jobs(paths, schemas);
    }
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
                { "name": "limit", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 50 } }
            ],
            "responses": {
                "200": response("Recent jobs", &json!({ "type": "object", "required": ["data"], "properties": { "data": { "type": "array", "items": schema_ref("AdminJob") } } })),
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
