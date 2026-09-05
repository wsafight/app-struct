use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(
    paths: &mut Map<String, Value>,
    schemas: &mut Map<String, Value>,
    mail_enabled: bool,
    file_enabled: bool,
) {
    if mail_enabled {
        schemas.insert("AdminMailSummary".to_owned(), mail_schema(false));
        schemas.insert("AdminMailDelivery".to_owned(), mail_schema(true));
        schemas.insert(
            "AdminMailList".to_owned(),
            list_schema("AdminMailSummary", false),
        );
        paths.insert(
            "/api/admin/mail".to_owned(),
            list_path("mail", "AdminMailList"),
        );
        paths.insert(
            "/api/admin/mail/{id}".to_owned(),
            detail_path("getAdminMailDelivery", "Mail delivery", "AdminMailDelivery"),
        );
    }
    if file_enabled {
        schemas.insert("AdminFile".to_owned(), file_schema());
        schemas.insert("AdminFileList".to_owned(), list_schema("AdminFile", true));
        paths.insert(
            "/api/admin/files".to_owned(),
            list_path("files", "AdminFileList"),
        );
        paths.insert(
            "/api/admin/files/{id}".to_owned(),
            detail_path("getAdminFile", "File metadata", "AdminFile"),
        );
    }
}

fn mail_schema(detail: bool) -> Value {
    let mut required = vec![
        "id",
        "provider",
        "template",
        "sender",
        "recipient",
        "subject",
        "tenant_id",
        "created_at",
    ];
    let mut properties = json!({
        "id": { "type": "string", "format": "uuid" },
        "provider": { "type": "string" },
        "template": { "type": "string" },
        "sender": { "type": "string" },
        "recipient": { "type": "string" },
        "subject": { "type": "string" },
        "tenant_id": { "type": ["string", "null"], "format": "uuid" },
        "created_at": { "type": "string", "format": "date-time" }
    });
    if detail {
        required.extend(["text_body", "html_body"]);
        properties["text_body"] = json!({ "type": "string" });
        properties["html_body"] = json!({ "type": ["string", "null"] });
    }
    json!({ "type": "object", "required": required, "properties": properties })
}

fn file_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "object_key", "original_name", "content_type", "size", "checksum", "tenant_id", "created_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "object_key": { "type": "string" },
            "original_name": { "type": "string" },
            "content_type": { "type": "string" },
            "size": { "type": "integer", "minimum": 0 },
            "checksum": { "type": "string" },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "created_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn list_schema(item: &str, include_bytes: bool) -> Value {
    let mut required = vec!["data", "meta"];
    let mut properties = json!({
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
    });
    if include_bytes {
        required.push("total_bytes");
        properties["total_bytes"] = json!({ "type": "integer", "minimum": 0 });
    }
    json!({ "type": "object", "required": required, "properties": properties })
}

fn list_path(name: &str, schema: &str) -> Value {
    let operation = if name == "mail" {
        "listAdminMail"
    } else {
        "listAdminFiles"
    };
    json!({
        "get": {
            "operationId": operation, "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [
                { "name": "search", "in": "query", "schema": { "type": "string", "maxLength": 200 } },
                page_parameter(), page_size_parameter()
            ],
            "responses": {
                "200": response("Administrative storage records", &schema_ref(schema)),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response()
            }
        }
    })
}

fn detail_path(operation: &str, description: &str, schema: &str) -> Value {
    json!({
        "get": {
            "operationId": operation, "tags": ["Admin"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [{
                "name": "id", "in": "path", "required": true,
                "schema": { "type": "string", "format": "uuid" }
            }],
            "responses": {
                "200": response(description, &schema_ref(schema)),
                "400": error_response(), "401": error_response(), "403": error_response(),
                "404": error_response()
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
