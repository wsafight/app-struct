use super::{error_response, request_body, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("TenantOrganization".to_owned(), organization_schema());
    schemas.insert("TenantInvitation".to_owned(), invitation_schema());
    schemas.insert(
        "TenantInvitationList".to_owned(),
        json!({
            "type": "object",
            "required": ["data"],
            "properties": { "data": { "type": "array", "items": schema_ref("TenantInvitation") } }
        }),
    );
    schemas.insert(
        "CreateTenantInvitationInput".to_owned(),
        json!({
            "type": "object",
            "required": ["email"],
            "properties": {
                "email": { "type": "string", "format": "email", "maxLength": 320 },
                "role": { "type": "string", "enum": ["member"], "default": "member" }
            }
        }),
    );
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
    add_invitation_paths(paths);
}

fn add_invitation_paths(paths: &mut Map<String, Value>) {
    paths.insert(
        "/api/tenant/invitations".to_owned(),
        json!({
            "get": {
                "operationId": "listTenantInvitations",
                "tags": ["Tenant"],
                "security": [{ "cookieSession": [] }],
                "parameters": [parameter()],
                "responses": { "200": response("Organization invitations", &schema_ref("TenantInvitationList")), "401": error_response(), "403": error_response() }
            },
            "post": {
                "operationId": "inviteTenantMember",
                "tags": ["Tenant"],
                "security": [{ "cookieSession": [] }],
                "parameters": [parameter()],
                "requestBody": request_body("CreateTenantInvitationInput"),
                "responses": { "201": response("Invitation created", &schema_ref("TenantInvitation")), "401": error_response(), "403": error_response(), "422": error_response() }
            }
        }),
    );
    paths.insert(
        "/api/tenant/invitations/{id}".to_owned(),
        json!({
            "delete": {
                "operationId": "revokeTenantInvitation",
                "tags": ["Tenant"],
                "security": [{ "cookieSession": [] }],
                "parameters": [parameter(), { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Invitation revoked" }, "400": error_response(), "401": error_response(), "403": error_response() }
            }
        }),
    );
    paths.insert(
        "/api/tenant/invitations/{token}/accept".to_owned(),
        json!({
            "post": {
                "operationId": "acceptTenantInvitation",
                "tags": ["Tenant"],
                "security": [{ "cookieSession": [] }],
                "parameters": [{ "name": "token", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": response("Invitation accepted", &schema_ref("TenantOrganization")), "400": error_response(), "401": error_response(), "403": error_response() }
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

fn invitation_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "email", "role", "expires_at", "accepted_at", "created_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "email": { "type": "string", "format": "email" },
            "role": { "type": "string", "enum": ["member"] },
            "expires_at": { "type": "string", "format": "date-time" },
            "accepted_at": { "type": ["string", "null"], "format": "date-time" },
            "created_at": { "type": "string", "format": "date-time" }
        }
    })
}
