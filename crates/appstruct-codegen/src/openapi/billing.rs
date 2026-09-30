use super::{error_response, request_body, response, schema_ref};
use appstruct_ir::AppIr;
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>, ir: &AppIr) {
    schemas.insert("BillingPlan".to_owned(), json!({
        "type": "object", "required": ["id", "trial_days", "entitlements"],
        "properties": { "id": { "type": "string" }, "trial_days": { "type": ["integer", "null"] }, "entitlements": { "type": "array", "items": { "type": "string" } } }
    }));
    schemas.insert("BillingSubscription".to_owned(), json!({
        "type": "object", "required": ["plan_id", "status", "current_period_end", "cancel_at_period_end"],
        "properties": { "plan_id": { "type": "string" }, "status": { "type": "string" }, "current_period_end": { "type": ["string", "null"], "format": "date-time" }, "cancel_at_period_end": { "type": "boolean" } }
    }));
    schemas.insert("BillingOverview".to_owned(), json!({
        "type": "object", "required": ["plans", "subscription", "entitlements", "customer_portal"],
        "properties": { "plans": { "type": "array", "items": schema_ref("BillingPlan") }, "subscription": { "anyOf": [schema_ref("BillingSubscription"), { "type": "null" }] }, "entitlements": { "type": "array", "items": { "type": "string" } }, "customer_portal": { "type": "boolean" } }
    }));
    schemas.insert("BillingCheckoutInput".to_owned(), json!({
        "type": "object", "required": ["plan_id"], "properties": { "plan_id": { "type": "string", "enum": ir.billing.plans.iter().map(|plan| plan.id.clone()).collect::<Vec<_>>() } }
    }));
    schemas.insert("BillingRedirect".to_owned(), json!({
        "type": "object", "required": ["url"], "properties": { "url": { "type": "string", "format": "uri" } }
    }));
    schemas.insert("BillingEvent".to_owned(), json!({
        "type": "object", "required": ["event_id", "event_type", "payload", "created_at"],
        "properties": {
            "event_id": { "type": "string" }, "event_type": { "type": "string" },
            "payload": { "type": "object" }, "created_at": { "type": "string", "format": "date-time" }
        }
    }));
    schemas.insert("BillingEventList".to_owned(), json!({
        "type": "object", "required": ["data", "meta"],
        "properties": {
            "data": { "type": "array", "items": schema_ref("BillingEvent") },
            "meta": { "type": "object", "required": ["page", "page_size", "total"], "properties": {
                "page": { "type": "integer" }, "page_size": { "type": "integer" }, "total": { "type": "integer" }
            }}
        }
    }));
    paths.insert("/api/billing".to_owned(), json!({
        "get": { "operationId": "getBillingOverview", "tags": ["Billing"], "security": [{ "cookieSession": [] }], "responses": { "200": response("Billing overview", &schema_ref("BillingOverview")), "401": error_response(), "400": error_response() } }
    }));
    paths.insert("/api/billing/checkout".to_owned(), json!({
        "post": { "operationId": "createBillingCheckout", "tags": ["Billing"], "security": [{ "cookieSession": [] }], "requestBody": request_body("BillingCheckoutInput"), "responses": { "200": response("Stripe Checkout redirect", &schema_ref("BillingRedirect")), "401": error_response(), "403": error_response(), "422": error_response() } }
    }));
    paths.insert("/api/billing/portal".to_owned(), json!({
        "post": { "operationId": "createBillingPortal", "tags": ["Billing"], "security": [{ "cookieSession": [] }], "responses": { "200": response("Stripe Customer Portal redirect", &schema_ref("BillingRedirect")), "401": error_response(), "403": error_response(), "404": error_response() } }
    }));
    paths.insert("/api/billing/stripe/webhook".to_owned(), json!({
        "post": { "operationId": "receiveStripeWebhook", "tags": ["Billing"], "security": [], "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object" } } } }, "responses": { "204": { "description": "Event accepted" }, "400": error_response(), "403": error_response() } }
    }));
    paths.insert("/api/admin/billing/events".to_owned(), json!({
        "get": {
            "operationId": "listAdminBillingEvents", "tags": ["Admin", "Billing"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [
                { "name": "page", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 1 } },
                { "name": "page_size", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 } }
            ],
            "responses": { "200": response("Billing webhook events", &schema_ref("BillingEventList")), "401": error_response(), "403": error_response() }
        }
    }));
    paths.insert("/api/admin/billing/events/{event_id}/replay".to_owned(), json!({
        "post": {
            "operationId": "replayAdminBillingEvent", "tags": ["Admin", "Billing"],
            "security": [{ "cookieSession": [] }, { "bearerToken": [] }],
            "parameters": [csrf_parameter(), { "name": "event_id", "in": "path", "required": true, "schema": { "type": "string" } }],
            "responses": { "200": response("Billing event replayed", &schema_ref("BillingEvent")), "401": error_response(), "403": error_response(), "404": error_response(), "409": error_response() }
        }
    }));
}

fn csrf_parameter() -> Value {
    json!({ "name": "X-CSRF-Token", "in": "header", "required": true, "schema": { "type": "string" } })
}
