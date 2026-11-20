use super::{auth, error_response, if_match_parameter, schema_ref, tenant, versioned_response};
use appstruct_ir::{AppIr, EntityIr};
use serde_json::{Map, Value, json};

pub(super) fn add_paths(paths: &mut Map<String, Value>, ir: &AppIr, parent: &EntityIr) {
    for aggregate in &parent.views.aggregates {
        let child = ir
            .entities
            .iter()
            .find(|entity| entity.id == aggregate.child)
            .expect("validated child");
        let relation = &child
            .fields
            .iter()
            .find(|field| field.id == aggregate.relation)
            .expect("validated relation")
            .rust_name;
        let input = |update| {
            let mut schema = super::schema::input_schema(child, update);
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove(relation);
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|field| field.as_str() != Some(relation));
            schema["additionalProperties"] = json!(false);
            schema
        };
        let revision = json!({ "type": "integer", "format": "int64", "minimum": 1 });
        let id = json!({ "type": "string", "format": "uuid" });
        let batch = json!({ "type": "object", "additionalProperties": false, "description": "1 to max_items operations in total; duplicate IDs/keys rejected. Atomic transaction.", "properties": {
            "creates": { "type": "array", "maxItems": aggregate.max_items, "items": { "type": "object", "additionalProperties": false, "required": ["key", "input"], "properties": { "key": { "type": "string", "minLength": 1, "maxLength": 128 }, "input": input(false) } } },
            "updates": { "type": "array", "maxItems": aggregate.max_items, "items": { "type": "object", "additionalProperties": false, "required": ["id", "revision", "input"], "properties": { "id": id, "revision": revision, "input": input(true) } } },
            "deletes": { "type": "array", "maxItems": aggregate.max_items, "items": { "type": "object", "additionalProperties": false, "required": ["id", "revision"], "properties": { "id": id, "revision": revision } } }
        }});
        let response = json!({ "type": "object", "required": ["parent", "rows", "created"], "properties": {
            "parent": schema_ref(&parent.rust_name), "rows": { "type": "array", "maxItems": aggregate.max_items, "items": schema_ref(&child.rust_name) },
            "created": { "type": "object", "additionalProperties": { "type": "string", "format": "uuid" } }
        }});
        let mut parameters =
            vec![json!({ "name": "id", "in": "path", "required": true, "schema": id })];
        if parent.tenant_scoped {
            parameters.push(tenant::parameter());
        }
        let mut writes = parameters.clone();
        writes.push(if_match_parameter());
        let mut create_parameters = Vec::new();
        if parent.tenant_scoped {
            create_parameters.push(tenant::parameter());
        }
        let create_body = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["parent"],
            "properties": {
                "parent": schema_ref(&format!("Create{}Input", parent.rust_name)),
                "creates": {
                    "type": "array",
                    "maxItems": aggregate.max_items,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["key", "input"],
                        "properties": {
                            "key": { "type": "string", "minLength": 1, "maxLength": 128 },
                            "input": input(false)
                        }
                    }
                }
            }
        });
        paths.insert(format!("/api/{}/_aggregates/{}", parent.table_name, aggregate.name), json!({
            "post": {
                "operationId": format!("create{}WithCollection_{}", parent.rust_name, aggregate.name),
                "tags": [&parent.rust_name],
                "parameters": create_parameters,
                "security": auth::security(&parent.access.create),
                "requestBody": { "required": true, "content": { "application/json": { "schema": create_body } } },
                "responses": {
                    "201": versioned_response("Parent and initial collection committed atomically", &response),
                    "403": error_response(),
                    "409": error_response(),
                    "422": error_response()
                }
            }
        }));
        paths.insert(format!("/api/{}/{{id}}/_aggregates/{}", parent.table_name, aggregate.name), json!({
            "get": { "operationId": format!("read{}Collection_{}", parent.rust_name, aggregate.name), "tags": [&parent.rust_name], "parameters": parameters, "security": auth::security(&parent.access.read), "responses": { "200": versioned_response("Authorized collection", &response), "403": error_response(), "404": error_response(), "422": error_response() } },
            "post": { "operationId": format!("save{}Collection_{}", parent.rust_name, aggregate.name), "tags": [&parent.rust_name], "parameters": writes, "security": auth::security(&parent.access.update), "requestBody": { "required": true, "content": { "application/json": { "schema": batch } } }, "responses": { "200": versioned_response("Collection committed", &response), "403": error_response(), "404": error_response(), "409": error_response(), "412": error_response(), "422": error_response(), "428": error_response() } }
        }));
    }
    prune_aggregate_child_mutations(paths, ir, parent);
}

fn prune_aggregate_child_mutations(paths: &mut Map<String, Value>, ir: &AppIr, parent: &EntityIr) {
    if ir
        .entities
        .iter()
        .flat_map(|entity| &entity.views.aggregates)
        .any(|aggregate| aggregate.child == parent.id)
    {
        let prefix = format!("/api/{}/", parent.table_name);
        for (_, path) in paths
            .iter_mut()
            .filter(|(path, _)| path.starts_with(&prefix))
        {
            if let Some(path) = path.as_object_mut() {
                for method in ["post", "patch", "delete"] {
                    path.remove(method);
                }
            }
        }
        paths.retain(|_, path| {
            path.as_object().is_none_or(|path| {
                path.keys()
                    .any(|key| ["get", "post", "patch", "delete", "put"].contains(&key.as_str()))
            })
        });
    }
}
