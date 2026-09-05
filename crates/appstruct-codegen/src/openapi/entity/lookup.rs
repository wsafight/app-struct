use super::{auth, error_response, response, schema_ref, tenant};
use appstruct_ir::EntityIr;
use serde_json::{Map, Value, json};

pub(super) fn add_path(paths: &mut Map<String, Value>, entity: &EntityIr) {
    let mut parameters = vec![json!({ "name": "ids", "in": "query", "required": true,
        "style": "form", "explode": false, "schema": { "type": "array", "minItems": 1, "maxItems": 100, "items": super::schema::primary_key(entity) } })];
    if entity.tenant_scoped {
        parameters.push(tenant::parameter());
    }
    paths.insert(format!("/api/{}/_lookup", entity.table_name), json!({ "get": {
        "operationId": format!("lookup{}", entity.rust_name), "tags": [&entity.rust_name],
        "security": auth::security(&entity.access.read), "parameters": parameters,
        "responses": { "200": response("Authorized records; unavailable IDs omitted", &json!({ "type": "array", "maxItems": 100, "items": schema_ref(&entity.rust_name) })), "400": error_response(), "401": error_response(), "403": error_response() }
    }}));
}
