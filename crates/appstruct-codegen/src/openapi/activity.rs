use super::{error_response, response, schema_ref};
use appstruct_ir::AppIr;
use serde_json::{Map, Value, json};

pub(super) fn add(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("ActivityEntry".to_owned(), entry_schema());
    schemas.insert("ActivityEntryList".to_owned(), list_schema());
    schemas.insert(
        "CreateActivityCommentInput".to_owned(),
        create_schema(ir.activity.attachments, ir.activity.max_comment_bytes),
    );
    schemas.insert("ModerateActivityInput".to_owned(), moderate_schema());
    paths.insert(
        "/api/activity/{resource}/{record_id}".to_owned(),
        list_path(ir),
    );
    paths.insert(
        "/api/activity/{resource}/{record_id}/comments".to_owned(),
        create_path(ir),
    );
    paths.insert(
        "/api/activity/{resource}/{record_id}/{entry_id}/withdraw".to_owned(),
        withdraw_path(ir),
    );
    paths.insert(
        "/api/activity/{resource}/{record_id}/{entry_id}/moderate".to_owned(),
        moderate_path(ir),
    );
    if ir.activity.attachments {
        paths.insert(
            "/api/activity/{resource}/{record_id}/{entry_id}/attachment".to_owned(),
            attachment_path(ir),
        );
    }
}

fn security() -> Value {
    json!([{ "cookieSession": [] }, { "bearerToken": [] }])
}

fn mutation_security() -> Value {
    json!([{ "cookieSession": [] }])
}

fn target_parameters(ir: &AppIr) -> Vec<Value> {
    vec![
        json!({
            "name": "resource", "in": "path", "required": true,
            "schema": { "type": "string", "enum": ir.activity.resources.iter().map(|entry| &entry.resource).collect::<Vec<_>>() }
        }),
        json!({
            "name": "record_id", "in": "path", "required": true,
            "schema": { "type": "string", "minLength": 1, "maxLength": 255 }
        }),
    ]
}

fn entry_parameters(ir: &AppIr) -> Vec<Value> {
    let mut parameters = target_parameters(ir);
    parameters.push(json!({
        "name": "entry_id", "in": "path", "required": true,
        "schema": { "type": "string", "format": "uuid" }
    }));
    parameters
}

fn list_path(ir: &AppIr) -> Value {
    let mut parameters = target_parameters(ir);
    parameters.extend([
        json!({ "name": "cursor", "in": "query", "schema": { "type": "string" } }),
        json!({ "name": "limit", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100 } }),
    ]);
    json!({ "get": {
        "operationId": "listActivityEntries", "tags": ["Activity"], "security": security(),
        "parameters": parameters,
        "responses": {
            "200": response("Record activity timeline", &schema_ref("ActivityEntryList")),
            "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
        }
    }})
}

fn create_path(ir: &AppIr) -> Value {
    json!({ "post": {
        "operationId": "createActivityComment", "tags": ["Activity"], "security": mutation_security(),
        "parameters": target_parameters(ir),
        "requestBody": { "required": true, "content": { "application/json": { "schema": schema_ref("CreateActivityCommentInput") } } },
        "responses": {
            "200": response("Created activity comment", &schema_ref("ActivityEntry")),
            "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response(), "422": error_response()
        }
    }})
}

fn withdraw_path(ir: &AppIr) -> Value {
    json!({ "post": {
        "operationId": "withdrawActivityComment", "tags": ["Activity"], "security": mutation_security(),
        "parameters": entry_parameters(ir),
        "responses": {
            "200": response("Withdrawn activity comment", &schema_ref("ActivityEntry")),
            "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response(), "409": error_response()
        }
    }})
}

fn moderate_path(ir: &AppIr) -> Value {
    json!({ "post": {
        "operationId": "moderateActivityComment", "tags": ["Activity"], "security": mutation_security(),
        "parameters": entry_parameters(ir),
        "requestBody": { "required": true, "content": { "application/json": { "schema": schema_ref("ModerateActivityInput") } } },
        "responses": {
            "200": response("Moderated activity comment", &schema_ref("ActivityEntry")),
            "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response(), "409": error_response(), "422": error_response()
        }
    }})
}

fn attachment_path(ir: &AppIr) -> Value {
    json!({ "get": {
        "operationId": "downloadActivityAttachment", "tags": ["Activity"], "security": security(),
        "parameters": entry_parameters(ir),
        "responses": {
            "200": { "description": "Activity attachment", "content": { "application/octet-stream": { "schema": { "type": "string", "contentEncoding": "binary" } } } },
            "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
        }
    }})
}

fn entry_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "resource", "record_id", "tenant_id", "actor_id", "kind", "body", "event", "payload", "attachment_file_id", "attachment_name", "attachment_content_type", "withdrawn_at", "withdrawn_by", "governance_reason", "occurred_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "resource": { "type": "string" }, "record_id": { "type": "string" },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "actor_id": { "type": ["string", "null"], "format": "uuid" },
            "kind": { "enum": ["comment", "system"] },
            "body": { "type": ["string", "null"] }, "event": { "type": ["string", "null"] },
            "payload": { "type": ["object", "null"] },
            "attachment_file_id": { "type": ["string", "null"], "format": "uuid" },
            "attachment_name": { "type": ["string", "null"] },
            "attachment_content_type": { "type": ["string", "null"] },
            "withdrawn_at": { "type": ["string", "null"], "format": "date-time" },
            "withdrawn_by": { "type": ["string", "null"], "format": "uuid" },
            "governance_reason": { "type": ["string", "null"] },
            "occurred_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn list_schema() -> Value {
    json!({
        "type": "object", "required": ["data", "meta"],
        "properties": {
            "data": { "type": "array", "items": schema_ref("ActivityEntry") },
            "meta": { "type": "object", "required": ["limit", "next_cursor", "has_more"], "properties": {
                "limit": { "type": "integer" }, "next_cursor": { "type": ["string", "null"] }, "has_more": { "type": "boolean" }
            } }
        }
    })
}

fn create_schema(attachments: bool, max_comment_bytes: u32) -> Value {
    let mut properties = Map::new();
    properties.insert(
        "body".to_owned(),
        json!({ "type": "string", "minLength": 1, "maxLength": max_comment_bytes }),
    );
    if attachments {
        properties.insert("attachment".to_owned(), json!({
            "type": ["object", "null"], "required": ["name", "content_type", "content_base64"],
            "properties": {
                "name": { "type": "string", "minLength": 1, "maxLength": 255 },
                "content_type": { "type": "string" }, "content_base64": { "type": "string", "contentEncoding": "base64" }
            },
            "additionalProperties": false
        }));
    }
    json!({ "type": "object", "required": ["body"], "properties": properties, "additionalProperties": false })
}

fn moderate_schema() -> Value {
    json!({
        "type": "object", "required": ["reason"],
        "properties": { "reason": { "type": "string", "minLength": 1, "maxLength": 1000 } },
        "additionalProperties": false
    })
}
