use super::{error_response, request_body, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("TenantOrganization".to_owned(), organization_schema());
    schemas.insert(
        "TenantOrganizationList".to_owned(),
        json!({
            "type": "object",
            "required": ["data"],
            "properties": {
                "data": { "type": "array", "items": schema_ref("TenantOrganization") }
            }
        }),
    );
    schemas.insert(
        "CreateTenantOrganizationInput".to_owned(),
        json!({
            "type": "object",
            "required": ["name"],
            "properties": { "name": { "type": "string", "minLength": 1, "maxLength": 120 } }
        }),
    );
    paths.insert(
        "/api/tenant/organizations".to_owned(),
        json!({
            "get": {
                "operationId": "listTenantOrganizations",
                "tags": ["Tenant"],
                "security": [{ "cookieSession": [] }],
                "responses": {
                    "200": response("Actor organizations", &schema_ref("TenantOrganizationList")),
                    "401": error_response()
                }
            },
            "post": {
                "operationId": "createTenantOrganization",
                "tags": ["Tenant"],
                "security": [{ "cookieSession": [] }],
                "requestBody": request_body("CreateTenantOrganizationInput"),
                "responses": {
                    "201": response("Organization created", &schema_ref("TenantOrganization")),
                    "401": error_response(),
                    "403": error_response(),
                    "422": error_response()
                }
            }
        }),
    );
}

pub(super) fn parameter() -> Value {
    json!({
        "name": "X-AppStruct-Tenant",
        "in": "header",
        "required": true,
        "schema": { "type": "string", "format": "uuid" }
    })
}

fn organization_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "name", "role", "created_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "name": { "type": "string" },
            "role": { "type": "string", "enum": ["owner", "member"] },
            "created_at": { "type": "string", "format": "date-time" }
        }
    })
}
