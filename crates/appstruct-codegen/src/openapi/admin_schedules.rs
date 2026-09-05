use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("AdminSchedule".to_owned(), schedule_schema());
    schemas.insert(
        "AdminScheduleList".to_owned(),
        json!({
            "type": "object", "required": ["data"],
            "properties": {
                "data": { "type": "array", "items": schema_ref("AdminSchedule") }
            }
        }),
    );
    schemas.insert(
        "AdminScheduleTrigger".to_owned(),
        json!({
            "type": "object", "required": ["job_id"],
            "properties": { "job_id": { "type": "string", "format": "uuid" } }
        }),
    );
    paths.insert("/api/admin/schedules".to_owned(), list_path());
    for (path, operation, description) in [
        (
            "/api/admin/schedules/{id}/pause",
            "pauseAdminSchedule",
            "Schedule paused",
        ),
        (
            "/api/admin/schedules/{id}/resume",
            "resumeAdminSchedule",
            "Schedule resumed",
        ),
    ] {
        paths.insert(
            path.to_owned(),
            mutation_path(operation, description, "AdminSchedule", "200"),
        );
    }
    paths.insert(
        "/api/admin/schedules/{id}/trigger".to_owned(),
        mutation_path(
            "triggerAdminSchedule",
            "Schedule job queued",
            "AdminScheduleTrigger",
            "201",
        ),
    );
}

fn schedule_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "name", "cron", "interval_seconds", "queue", "kind", "enabled", "paused", "next_run_at", "last_run_at", "created_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "name": { "type": "string" },
            "cron": { "type": "string" },
            "interval_seconds": { "type": ["integer", "null"], "minimum": 1 },
            "queue": { "type": "string" },
            "kind": { "type": "string" },
            "enabled": { "type": "boolean" },
            "paused": { "type": "boolean" },
            "next_run_at": { "type": "string", "format": "date-time" },
            "last_run_at": { "type": ["string", "null"], "format": "date-time" },
            "created_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn list_path() -> Value {
    json!({
        "get": {
            "operationId": "listAdminSchedules", "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "responses": {
                "200": response("Schedule definitions", &schema_ref("AdminScheduleList")),
                "401": error_response(), "403": error_response(), "404": error_response()
            }
        }
    })
}

fn mutation_path(operation: &str, description: &str, schema: &str, status: &str) -> Value {
    json!({
        "post": {
            "operationId": operation, "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [
                {
                    "name": "X-CSRF-Token", "in": "header", "required": true,
                    "schema": { "type": "string" }
                },
                {
                    "name": "id", "in": "path", "required": true,
                    "schema": { "type": "string", "format": "uuid" }
                }
            ],
            "responses": {
                (status): response(description, &schema_ref(schema)),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response()
            }
        }
    })
}
