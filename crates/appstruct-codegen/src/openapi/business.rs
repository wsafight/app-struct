use super::{error_response, response, schema_ref};
use serde_json::{Map, Value, json};

pub(super) fn add(paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert(
        "BusinessMetric".to_owned(),
        json!({
            "type": "object",
            "required": ["key", "value", "definition", "source", "window", "unavailable_reason"],
            "properties": {
                "key": { "type": "string" },
                "value": { "type": ["integer", "null"], "minimum": 0 },
                "definition": { "type": "string" },
                "source": { "type": "string" },
                "window": { "type": "string" },
                "unavailable_reason": { "type": ["string", "null"] }
            }
        }),
    );
    schemas.insert(
        "BusinessOverview".to_owned(),
        json!({
            "type": "object", "required": ["generated_at", "metrics"],
            "properties": {
                "generated_at": { "type": "string", "format": "date-time" },
                "metrics": { "type": "array", "items": schema_ref("BusinessMetric") }
            }
        }),
    );
    paths.insert("/api/admin/business".to_owned(), json!({
        "get": {
            "operationId": "businessOverview", "tags": ["Admin"],
            "description": "Application-wide business indicators. Requires the admin role, not a tenant selection. Missing event sources return null values with an explanation.",
            "security": [{ "cookieSession": [] }],
            "responses": {
                "200": response("Business indicators", &schema_ref("BusinessOverview")),
                "401": error_response(), "403": error_response()
            }
        }
    }));
}
