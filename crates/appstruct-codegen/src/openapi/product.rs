use super::{error_response, response, schema_ref};
use appstruct_ir::AppIr;
use serde_json::{Map, Value, json};

pub(super) fn add(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    account(paths, schemas);
    engagement(paths);
    workspace(ir, paths);
}

fn account(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("ProductProfile".to_owned(), json!({ "type": "object", "required": ["display_name", "locale", "timezone", "notifications", "mfa_enabled"], "properties": { "display_name": { "type": "string", "maxLength": 120 }, "locale": { "enum": ["en-US", "zh-CN"] }, "timezone": { "enum": ["UTC", "Asia/Shanghai"] }, "notifications": { "type": "boolean" }, "mfa_enabled": { "type": "boolean" } } }));
    schemas.insert("ProductNotification".to_owned(), json!({ "type": "object", "required": ["id", "title", "body", "target", "read_at", "created_at", "tenant_id"], "properties": { "id": uuid(), "tenant_id": { "type": ["string", "null"], "format": "uuid" }, "title": { "type": "string" }, "body": { "type": "string" }, "target": { "type": "string", "pattern": "^/[^/]" }, "read_at": { "type": ["string", "null"], "format": "date-time" }, "created_at": { "type": "string", "format": "date-time" } } }));
    operation(
        paths,
        "/api/auth/settings",
        "get",
        "getAccountSettings",
        None,
        Some(schema_ref("ProductProfile")),
        false,
    );
    let mut profile = schemas["ProductProfile"].clone();
    profile["properties"]
        .as_object_mut()
        .unwrap()
        .remove("mfa_enabled");
    profile["required"] = json!(["display_name", "locale", "timezone", "notifications"]);
    profile["additionalProperties"] = json!(false);
    operation(
        paths,
        "/api/auth/settings",
        "patch",
        "updateAccountSettings",
        Some(profile),
        None,
        false,
    );
    let proof = json!({ "type": "object", "required": ["password"], "additionalProperties": false, "properties": { "password": { "type": "string", "writeOnly": true }, "code": { "type": "string", "writeOnly": true } } });
    for (path, name, extra) in [
        (
            "password",
            "changeAccountPassword",
            Some((
                "new_password",
                json!({ "type": "string", "minLength": 12, "writeOnly": true }),
            )),
        ),
        (
            "email",
            "requestAccountEmailChange",
            Some(("email", json!({ "type": "string", "format": "email" }))),
        ),
        ("sessions/revoke", "revokeOwnSessions", None),
        ("mfa/disable", "disableAccountMfa", None),
    ] {
        let mut input = proof.clone();
        if let Some((field, schema)) = extra {
            input["properties"][field] = schema;
            input["required"].as_array_mut().unwrap().push(json!(field));
        }
        operation(
            paths,
            &format!("/api/auth/settings/{path}"),
            "post",
            name,
            Some(input),
            None,
            false,
        );
    }
    operation(
        paths,
        "/api/auth/settings/mfa/setup",
        "post",
        "setupAccountMfa",
        Some(proof),
        Some(
            json!({ "type": "object", "required": ["secret", "uri"], "properties": { "secret": { "type": "string" }, "uri": { "type": "string" } } }),
        ),
        false,
    );
    operation(
        paths,
        "/api/auth/settings/mfa/enable",
        "post",
        "enableAccountMfa",
        Some(
            json!({ "type": "object", "required": ["code"], "additionalProperties": false, "properties": { "code": { "type": "string", "writeOnly": true } } }),
        ),
        Some(
            json!({ "type": "object", "required": ["recovery_codes", "reauthenticate"], "properties": { "recovery_codes": { "type": "array", "items": { "type": "string" } }, "reauthenticate": { "type": "boolean" } } }),
        ),
        false,
    );
    operation(
        paths,
        "/api/auth/settings/email/confirm",
        "post",
        "confirmAccountEmailChange",
        Some(
            json!({ "type": "object", "required": ["token"], "additionalProperties": false, "properties": { "token": { "type": "string", "writeOnly": true } } }),
        ),
        None,
        false,
    );
}

fn engagement(paths: &mut Map<String, Value>) {
    operation(
        paths,
        "/api/product/notifications",
        "get",
        "listNotifications",
        None,
        Some(
            json!({ "type": "object", "required": ["data", "total", "unread"], "properties": { "data": { "type": "array", "items": schema_ref("ProductNotification") }, "total": { "type": "integer" }, "unread": { "type": "integer" } } }),
        ),
        true,
    );
    operation(
        paths,
        "/api/product/notifications/read",
        "post",
        "readAllNotifications",
        None,
        None,
        true,
    );
    operation(
        paths,
        "/api/product/notifications/{id}/read",
        "post",
        "readNotification",
        None,
        None,
        true,
    );
    operation(
        paths,
        "/api/product/visits",
        "post",
        "recordConsentedVisit",
        Some(
            json!({ "type": "object", "required": ["visitor_id", "page", "consent"], "additionalProperties": false, "properties": { "visitor_id": uuid(), "page": { "enum": ["/", "/pricing"] }, "consent": { "const": true } } }),
        ),
        None,
        false,
    );
    paths["/api/product/visits"]["post"]["security"] = json!([]);
    paths["/api/product/visits"]["post"]["parameters"] = json!([]);
    operation(
        paths,
        "/api/product/insights",
        "get",
        "productInsights",
        None,
        Some(
            json!({ "type": "object", "description": "Admin-only 30-day event counts, fully observed D7 retention, and captured payments/refunds grouped by currency. Minor-unit monetary sums are strings; these are not bank settlement or MRR." }),
        ),
        false,
    );
}

fn workspace(ir: &AppIr, paths: &mut Map<String, Value>) {
    if ir.tenant.enabled {
        for (path, method, name, input, output) in [
            (
                "/api/product/team",
                "get",
                "getTeamSettings",
                None,
                Some(
                    json!({ "type": "object", "required": ["id", "name", "is_owner", "seat_limit", "seats_used", "members", "members_truncated"], "properties": { "id": uuid(), "name": { "type": "string" }, "is_owner": { "type": "boolean" }, "seat_limit": { "type": "integer", "description": "0 means unlimited" }, "seats_used": { "type": "integer" }, "members_truncated": { "type": "boolean" }, "members": { "type": "array", "items": { "type": "object" }, "maxItems": 1000 } } }),
                ),
            ),
            (
                "/api/product/team",
                "patch",
                "renameTeam",
                Some(
                    json!({ "type": "object", "required": ["name"], "additionalProperties": false, "properties": { "name": { "type": "string", "minLength": 1, "maxLength": 120 } } }),
                ),
                None,
            ),
            (
                "/api/product/team/transfer",
                "post",
                "transferTeamOwnership",
                Some(
                    json!({ "type": "object", "required": ["member_id"], "additionalProperties": false, "properties": { "member_id": uuid() } }),
                ),
                None,
            ),
            (
                "/api/product/team/members/{id}",
                "delete",
                "removeTeamMember",
                None,
                None,
            ),
            (
                "/api/product/credits",
                "get",
                "getCreditBalance",
                None,
                Some(
                    json!({ "type": "object", "required": ["monthly_allowance", "monthly_remaining", "paid_balance", "available", "resets_at", "data", "page_size"], "properties": { "monthly_allowance": { "type": "integer" }, "monthly_remaining": { "type": "integer" }, "paid_balance": { "type": "integer" }, "available": { "type": "integer" }, "resets_at": { "type": "string", "format": "date-time" }, "page_size": { "type": "integer" }, "data": { "type": "array", "items": { "type": "object" } } } }),
                ),
            ),
        ] {
            operation(paths, path, method, name, input, output, true);
        }
    }
    if ir.billing.enabled {
        operation(
            paths,
            "/api/product/credits/checkout",
            "post",
            "checkoutCreditPack",
            None,
            Some(
                json!({ "type": "object", "required": ["url"], "properties": { "url": { "type": "string", "format": "uri" } } }),
            ),
            true,
        );
        paths["/api/product/credits/checkout"]["post"]["parameters"].as_array_mut().unwrap().push(json!({ "name": "Idempotency-Key", "in": "header", "required": true, "schema": uuid() }));
    }
}

fn uuid() -> Value {
    json!({ "type": "string", "format": "uuid" })
}

fn operation(
    paths: &mut Map<String, Value>,
    path: &str,
    method: &str,
    name: &str,
    input: Option<Value>,
    output: Option<Value>,
    scoped: bool,
) {
    let mut parameters = Vec::new();
    if path.contains("{id}") {
        parameters.push(json!({ "name": "id", "in": "path", "required": true, "schema": uuid() }));
    }
    if scoped {
        parameters.push(json!({ "name": "X-AppStruct-Tenant", "in": "header", "required": path.starts_with("/api/product/team") || path.starts_with("/api/product/credits"), "schema": uuid() }));
    }
    if method != "get" {
        parameters.push(json!({ "name": "X-CSRF-Token", "in": "header", "required": true, "schema": { "type": "string" } }));
    }
    if method == "get" && (path.ends_with("credits") || path.ends_with("notifications")) {
        parameters.extend([json!({ "name": "page", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 1 } }), json!({ "name": "page_size", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 } })]);
    }
    let mut operation = json!({ "operationId": name, "tags": ["Product"], "security": [{ "cookieSession": [] }], "parameters": parameters, "responses": { "400": error_response(), "401": error_response(), "402": error_response(), "403": error_response(), "404": error_response(), "409": error_response() } });
    if let Some(schema) = output {
        operation["responses"]["200"] = response("Success", &schema);
    } else {
        operation["responses"]["204"] = json!({ "description": "Completed" });
    }
    if let Some(schema) = input {
        operation["requestBody"] =
            json!({ "required": true, "content": { "application/json": { "schema": schema } } });
    }
    paths.entry(path.to_owned()).or_insert_with(|| json!({}))[method] = operation;
}
