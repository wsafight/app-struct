use super::{auth, error_response, if_match_parameter, request_body, response, schema_ref, tenant};
use appstruct_ir::{AppIr, EntityIr};
use serde_json::{Map, Value, json};

pub(super) fn add_paths(paths: &mut Map<String, Value>, _ir: &AppIr, entity: &EntityIr) {
    let name = &entity.rust_name;
    let base = format!("/api/{}/", entity.table_name);
    let security = auth::security(&entity.access.list);
    let mut bulk_parameters = Vec::new();
    if entity.tenant_scoped {
        bulk_parameters.push(tenant::parameter());
    }
    paths.insert(format!("{base}_bulk"), json!({
        "patch": { "operationId": format!("bulkUpdate{name}"), "tags": [name], "security": auth::security(&entity.access.update), "parameters": [if_match_parameter()], "requestBody": request_body(&format!("BulkUpdate{name}Input")), "responses": { "200": response("Bulk update result", &schema_ref("BulkResult")), "403": error_response() } },
        "delete": { "operationId": format!("bulkDelete{name}"), "tags": [name], "security": auth::security(&entity.access.delete), "parameters": [if_match_parameter()], "requestBody": request_body("BulkDeleteInput"), "responses": { "200": response("Bulk delete result", &schema_ref("BulkResult")), "403": error_response() } }
    }));
    paths.insert(format!("{base}_export.csv"), json!({ "get": { "operationId": format!("export{name}Csv"), "tags": [name], "security": security, "parameters": bulk_parameters, "responses": { "200": { "description": "CSV export", "content": { "text/csv": { "schema": { "type": "string" } } } }, "403": error_response() } } }));
    paths.insert(format!("{base}_import.csv"), json!({ "post": { "operationId": format!("import{name}Csv"), "tags": [name], "security": auth::security(&entity.access.create), "requestBody": { "required": true, "content": { "text/csv": { "schema": { "type": "string" } } } }, "responses": { "200": response("CSV import result", &schema_ref("BulkResult")), "403": error_response() } } }));
    if entity.views.soft_delete {
        paths.insert(format!("{base}_trash"), json!({ "get": { "operationId": format!("list{name}Trash"), "tags": [name], "security": security, "parameters": trash_parameters(), "responses": { "200": response("Trashed resources", &schema_ref(&format!("{name}ListResponse"))), "403": error_response() } } }));
        paths.insert(format!("{base}_restore"), json!({ "post": { "operationId": format!("restore{name}"), "tags": [name], "security": auth::security(&entity.access.update), "requestBody": request_body("BulkDeleteInput"), "responses": { "200": response("Restore result", &schema_ref("BulkResult")), "403": error_response() } } }));
    }
}

pub(super) fn add_schemas(schemas: &mut Map<String, Value>, entity: &EntityIr) {
    schemas.insert("BulkResult".to_owned(), bulk_result_schema());
    schemas.insert("BulkDeleteInput".to_owned(), bulk_delete_schema());
    schemas.insert(
        format!("BulkUpdate{}Input", entity.rust_name),
        bulk_update_schema(entity),
    );
}

fn trash_parameters() -> Vec<Value> {
    vec![
        query_parameter(
            "page",
            json!({ "type": "integer", "minimum": 1, "default": 1 }),
        ),
        query_parameter(
            "page_size",
            json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }),
        ),
    ]
}

fn query_parameter(name: &str, schema: Value) -> Value {
    json!({ "name": name, "in": "query", "required": false, "schema": schema })
}

fn bulk_result_schema() -> Value {
    json!({ "type": "object", "required": ["succeeded", "failed"], "properties": { "succeeded": { "type": "array", "items": { "type": "string" } }, "failed": { "type": "array", "items": { "$ref": "#/components/schemas/BulkFailure" } } } })
}

fn bulk_delete_schema() -> Value {
    json!({ "type": "object", "required": ["ids", "expected_revisions"], "properties": { "ids": { "type": "array", "items": { "type": "string" }, "minItems": 1 }, "expected_revisions": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 1 } } } })
}

fn bulk_update_schema(entity: &EntityIr) -> Value {
    json!({ "type": "object", "required": ["ids", "patch", "expected_revisions"], "properties": { "ids": { "type": "array", "items": { "type": "string" }, "minItems": 1 }, "patch": schema_ref(&format!("Update{}Input", entity.rust_name)), "expected_revisions": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 1 } } } })
}

pub(super) fn add_common_schemas(schemas: &mut Map<String, Value>) {
    schemas.insert("BulkFailure".to_owned(), json!({ "type": "object", "required": ["id", "code", "message"], "properties": { "id": { "type": "string" }, "code": { "type": "string" }, "message": { "type": "string" } } }));
}
