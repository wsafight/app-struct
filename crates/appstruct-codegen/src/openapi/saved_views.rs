use super::{error_response, request_body, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("SavedView".to_owned(), saved_view_schema());
    schemas.insert("CreateSavedViewInput".to_owned(), create_schema());
    schemas.insert("UpdateSavedViewInput".to_owned(), update_schema());
    schemas.insert(
        "SavedViewList".to_owned(),
        json!({
            "type": "object", "required": ["data"],
            "properties": { "data": { "type": "array", "items": schema_ref("SavedView") } }
        }),
    );
    paths.insert("/api/saved-views".to_owned(), collection_path());
    paths.insert("/api/saved-views/{id}".to_owned(), member_path());
}

fn saved_view_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "name", "query", "visibility", "revision", "owned", "created_at", "updated_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "name": { "type": "string", "minLength": 1, "maxLength": 80 },
            "query": { "type": "string", "maxLength": 4096 },
            "visibility": { "type": "string", "enum": ["private", "team"] },
            "revision": { "type": "integer", "minimum": 1 },
            "owned": { "type": "boolean" },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn create_schema() -> Value {
    let mut schema = update_schema();
    schema["required"] = json!(["resource", "name", "query", "visibility"]);
    schema["properties"]["resource"] = json!({ "type": "string" });
    schema
}

fn update_schema() -> Value {
    json!({
        "type": "object", "required": ["name", "query", "visibility"],
        "properties": {
            "name": { "type": "string", "minLength": 1, "maxLength": 80 },
            "query": { "type": "string", "maxLength": 4096 },
            "visibility": { "type": "string", "enum": ["private", "team"] }
        }
    })
}

fn collection_path() -> Value {
    json!({
        "get": {
            "operationId": "listSavedViews", "tags": ["Saved Views"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [{ "name": "resource", "in": "query", "required": true, "schema": { "type": "string" } }],
            "responses": {
                "200": response("Visible saved views", &schema_ref("SavedViewList")),
                "400": error_response(), "401": error_response(), "403": error_response()
            }
        },
        "post": {
            "operationId": "createSavedView", "tags": ["Saved Views"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter()],
            "requestBody": request_body("CreateSavedViewInput"),
            "responses": {
                "201": versioned_response("Saved view created"),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "409": error_response()
            }
        }
    })
}

fn member_path() -> Value {
    json!({
        "patch": {
            "operationId": "updateSavedView", "tags": ["Saved Views"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter(), id_parameter(), if_match_parameter()],
            "requestBody": request_body("UpdateSavedViewInput"),
            "responses": {
                "200": versioned_response("Saved view updated"),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response(), "409": error_response(), "412": error_response(),
                "428": error_response()
            }
        },
        "delete": {
            "operationId": "deleteSavedView", "tags": ["Saved Views"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter(), id_parameter(), if_match_parameter()],
            "responses": {
                "204": { "description": "Saved view deleted" },
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response(), "412": error_response(), "428": error_response()
            }
        }
    })
}

fn versioned_response(description: &str) -> Value {
    let mut value = response(description, &schema_ref("SavedView"));
    value["headers"] = json!({
        "ETag": {
            "description": "Optimistic concurrency revision",
            "schema": { "type": "string", "pattern": "^\\\"rev-[1-9][0-9]*\\\"$" }
        }
    });
    value
}

fn csrf_parameter() -> Value {
    json!({ "name": "X-CSRF-Token", "in": "header", "required": true, "schema": { "type": "string" } })
}

fn id_parameter() -> Value {
    json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } })
}

fn if_match_parameter() -> Value {
    json!({
        "name": "If-Match", "in": "header", "required": true,
        "schema": { "type": "string", "pattern": "^\\\"rev-[1-9][0-9]*\\\"$" }
    })
}
