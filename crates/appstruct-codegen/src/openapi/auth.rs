use super::{error_response, request_body, response, schema_ref};
use appstruct_ir::{AccessRuleIr, AppIr};
use serde_json::{Map, Value, json};

pub(super) fn security(rule: &AccessRuleIr) -> Value {
    if allows_anonymous(rule) {
        json!([])
    } else {
        json!([{ "cookieSession": [] }])
    }
}

pub(super) fn security_schemes(enabled: bool) -> Value {
    if enabled {
        json!({
            "cookieSession": {
                "type": "apiKey",
                "in": "cookie",
                "name": "appstruct_session"
            },
            "bearerToken": { "type": "http", "scheme": "bearer" }
        })
    } else {
        json!({})
    }
}

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>, ir: &AppIr) {
    schemas.insert("AuthCredentials".to_owned(), credentials_schema());
    schemas.insert("AuthUser".to_owned(), user_schema(ir));
    schemas.insert("AuthResponse".to_owned(), auth_response_schema());
    schemas.insert("PasswordResetRequest".to_owned(), reset_request_schema());
    schemas.insert("PasswordResetInput".to_owned(), reset_input_schema());
    schemas.insert(
        "EmailVerificationInput".to_owned(),
        email_verification_schema(),
    );
    paths.insert(
        "/api/auth/login".to_owned(),
        json!({ "post": auth_operation("login", "AuthCredentials") }),
    );
    if ir.auth.registration_enabled {
        paths.insert(
            "/api/auth/register".to_owned(),
            json!({ "post": auth_operation("register", "AuthCredentials") }),
        );
    }
    paths.insert(
        "/api/auth/logout".to_owned(),
        json!({
            "post": {
                "operationId": "logout",
                "tags": ["Auth"],
                "security": [{ "cookieSession": [] }],
                "parameters": [csrf_parameter()],
                "responses": {
                    "204": { "description": "Session revoked" },
                    "401": error_response(),
                    "403": error_response()
                }
            }
        }),
    );
    paths.insert(
        "/api/auth/me".to_owned(),
        json!({
            "get": {
                "operationId": "currentUser",
                "tags": ["Auth"],
                "security": [{ "cookieSession": [] }],
                "responses": {
                    "200": response("Current user", &schema_ref("AuthResponse")),
                    "401": error_response()
                }
            }
        }),
    );
    if ir.auth.password_reset_enabled {
        add_password_reset_paths(paths);
    }
    if ir.auth.oauth_enabled {
        add_oauth_paths(paths);
    }
    add_email_verification_paths(paths);
    add_token_paths(paths, schemas);
    super::admin::add(paths, schemas, ir.jobs.enabled, ir.webhooks.enabled);
    if ir.jobs.enabled {
        super::admin_schedules::add(paths, schemas);
    }
    super::admin_storage::add(paths, schemas, ir.mail.enabled, ir.file.enabled);
    super::saved_views::add(paths, schemas);
}

fn add_token_paths(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert(
        "CreateApiTokenInput".to_owned(),
        json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string", "minLength": 1, "maxLength": 80 },
                "expires_in_days": { "type": "integer", "minimum": 1, "maximum": 3650 }
            }
        }),
    );
    schemas.insert(
        "ApiToken".to_owned(),
        json!({
            "type": "object",
            "required": ["id", "name", "created_at", "last_used_at", "expires_at", "revoked_at"],
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "name": { "type": "string" },
                "created_at": { "type": "string", "format": "date-time" },
                "last_used_at": { "type": ["string", "null"], "format": "date-time" },
                "expires_at": { "type": ["string", "null"], "format": "date-time" },
                "revoked_at": { "type": ["string", "null"], "format": "date-time" }
            }
        }),
    );
    schemas.insert("CreatedApiToken".to_owned(), json!({
        "allOf": [schema_ref("ApiToken"), { "type": "object", "required": ["token"], "properties": { "token": { "type": "string" } } }]
    }));
    paths.insert("/api/auth/tokens".to_owned(), json!({
        "get": {
            "operationId": "listApiTokens", "tags": ["Auth"], "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "responses": { "200": response("Personal API tokens", &json!({ "type": "array", "items": schema_ref("ApiToken") })), "401": error_response() }
        },
        "post": {
            "operationId": "createApiToken", "tags": ["Auth"], "security": [{ "cookieSession": [] }], "parameters": [csrf_parameter()],
            "requestBody": request_body("CreateApiTokenInput"),
            "responses": { "201": response("Token created; plaintext is returned once", &schema_ref("CreatedApiToken")), "401": error_response(), "422": error_response() }
        }
    }));
    paths.insert("/api/auth/tokens/{id}".to_owned(), json!({
        "delete": {
            "operationId": "revokeApiToken", "tags": ["Auth"], "security": [{ "cookieSession": [] }, { "bearerToken": [] }], "parameters": [csrf_parameter(), { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
            "responses": { "204": { "description": "Token revoked" }, "400": error_response(), "401": error_response() }
        }
    }));
}

fn add_email_verification_paths(paths: &mut Map<String, Value>) {
    paths.insert(
        "/api/auth/email/request".to_owned(),
        json!({
            "post": {
                "operationId": "requestEmailVerification",
                "tags": ["Auth"],
                "security": [{ "cookieSession": [] }],
                "parameters": [csrf_parameter()],
                "responses": { "204": { "description": "Verification email queued" }, "401": error_response() }
            }
        }),
    );
    paths.insert(
        "/api/auth/email/verify".to_owned(),
        json!({
            "post": {
                "operationId": "verifyEmail",
                "tags": ["Auth"],
                "requestBody": request_body("EmailVerificationInput"),
                "responses": { "204": { "description": "Email verified" }, "400": error_response() }
            }
        }),
    );
}

fn add_oauth_paths(paths: &mut Map<String, Value>) {
    paths.insert(
        "/api/auth/oauth/oidc/start".to_owned(),
        json!({
            "get": {
                "operationId": "startOidcLogin",
                "tags": ["Auth"],
                "responses": { "307": { "description": "Redirect to OIDC provider" }, "404": error_response() }
            }
        }),
    );
    paths.insert(
        "/api/auth/oauth/oidc/callback".to_owned(),
        json!({
            "get": {
                "operationId": "oidcCallback",
                "tags": ["Auth"],
                "parameters": [
                    { "name": "code", "in": "query", "required": true, "schema": { "type": "string" } },
                    { "name": "state", "in": "query", "required": true, "schema": { "type": "string" } }
                ],
                "responses": { "307": { "description": "Redirect to application" }, "400": error_response(), "502": error_response() }
            }
        }),
    );
}

fn auth_operation(name: &str, body: &str) -> Value {
    json!({
        "operationId": name,
        "tags": ["Auth"],
        "requestBody": request_body(body),
        "responses": {
            "200": response("Authenticated session", &schema_ref("AuthResponse")),
            "400": error_response(),
            "401": error_response(),
            "404": error_response(),
            "429": error_response()
        }
    })
}

fn add_password_reset_paths(paths: &mut Map<String, Value>) {
    paths.insert(
        "/api/auth/password/request".to_owned(),
        json!({
            "post": {
                "operationId": "requestPasswordReset",
                "tags": ["Auth"],
                "requestBody": request_body("PasswordResetRequest"),
                "responses": {
                    "204": { "description": "Request accepted" },
                    "404": error_response(),
                    "429": error_response()
                }
            }
        }),
    );
    paths.insert(
        "/api/auth/password/reset".to_owned(),
        json!({
            "post": {
                "operationId": "resetPassword",
                "tags": ["Auth"],
                "requestBody": request_body("PasswordResetInput"),
                "responses": {
                    "204": { "description": "Password updated" },
                    "400": error_response(),
                    "404": error_response()
                }
            }
        }),
    );
}

fn credentials_schema() -> Value {
    json!({
        "type": "object",
        "required": ["email", "password"],
        "properties": {
            "email": { "type": "string", "format": "email", "maxLength": 320 },
            "password": {
                "type": "string",
                "format": "password",
                "minLength": 12,
                "maxLength": 1024
            }
        }
    })
}

fn user_schema(ir: &AppIr) -> Value {
    json!({
        "type": "object",
        "required": ["id", "email", "roles"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "email": { "type": "string", "format": "email" },
            "roles": {
                "type": "array",
                "items": { "type": "string", "enum": ir.auth.roles }
            }
        }
    })
}

fn auth_response_schema() -> Value {
    json!({
        "type": "object",
        "required": ["user", "email_verified"],
        "properties": {
            "user": schema_ref("AuthUser"),
            "email_verified": { "type": "boolean" }
        }
    })
}

fn reset_request_schema() -> Value {
    json!({
        "type": "object",
        "required": ["email"],
        "properties": { "email": { "type": "string", "format": "email" } }
    })
}

fn reset_input_schema() -> Value {
    json!({
        "type": "object",
        "required": ["token", "password"],
        "properties": {
            "token": { "type": "string" },
            "password": {
                "type": "string",
                "format": "password",
                "minLength": 12,
                "maxLength": 1024
            }
        }
    })
}

fn email_verification_schema() -> Value {
    json!({
        "type": "object",
        "required": ["token"],
        "properties": { "token": { "type": "string", "minLength": 16 } }
    })
}

fn csrf_parameter() -> Value {
    json!({
        "name": "X-CSRF-Token",
        "in": "header",
        "required": true,
        "schema": { "type": "string" }
    })
}

fn allows_anonymous(rule: &AccessRuleIr) -> bool {
    match rule {
        AccessRuleIr::Public => true,
        AccessRuleIr::Any { rules } => rules.iter().any(allows_anonymous),
        AccessRuleIr::All { rules } => rules.iter().all(allows_anonymous),
        AccessRuleIr::Authenticated | AccessRuleIr::Role { .. } | AccessRuleIr::Owner { .. } => {
            false
        }
    }
}
