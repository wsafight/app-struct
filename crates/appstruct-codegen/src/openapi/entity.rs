use super::{
    auth, error_response, if_match_parameter, request_body, response, schema_ref, tenant,
    versioned_response,
};
use appstruct_ir::{AccessRuleIr, AppIr, EntityIr, FieldTypeIr};
use serde_json::{Map, Value, json};

mod collections;
mod lookup;
mod schema;
pub(super) fn add_paths(paths: &mut Map<String, Value>, ir: &AppIr, entity: &EntityIr) {
    let singular = &entity.rust_name;
    let collection = format!("/api/{}/", entity.table_name);
    let member = format!("/api/{}/{{id}}", entity.table_name);
    let mut list_parameters = list_parameters(ir, entity);
    let mut member_parameters = vec![json!({
        "name": "id",
        "in": "path",
        "required": true,
        "schema": schema::primary_key(entity),
    })];
    let create_parameters = if entity.tenant_scoped {
        list_parameters.push(tenant::parameter());
        member_parameters.push(tenant::parameter());
        vec![tenant::parameter()]
    } else {
        Vec::new()
    };
    paths.insert(
        collection,
        json!({
            "get": {
                "operationId": format!("list{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.list),
                "parameters": list_parameters,
                "responses": {
                    "200": response(
                        "Paginated resource collection",
                        &schema_ref(&format!("{singular}ListResponse"))
                    )
                }
            },
            "post": {
                "operationId": format!("create{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.create),
                "parameters": create_parameters,
                "requestBody": request_body(&format!("Create{singular}Input")),
                "responses": {
                    "201": versioned_response("Resource created", &schema_ref(singular)),
                    "422": error_response(),
                }
            }
        }),
    );
    paths.insert(
        member,
        json!({
            "parameters": member_parameters,
            "get": {
                "operationId": format!("get{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.read),
                "responses": {
                    "200": versioned_response("Resource", &schema_ref(singular)),
                    "404": error_response(),
                }
            },
            "patch": {
                "operationId": format!("update{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.update),
                "parameters": [if_match_parameter()],
                "requestBody": request_body(&format!("Update{singular}Input")),
                "responses": {
                    "200": versioned_response("Resource updated", &schema_ref(singular)),
                    "404": error_response(),
                    "412": error_response(),
                    "422": error_response(),
                    "428": error_response(),
                }
            },
            "delete": {
                "operationId": format!("delete{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.delete),
                "parameters": [if_match_parameter()],
                "responses": {
                    "204": { "description": "Resource deleted" },
                    "404": error_response(),
                    "412": error_response(),
                    "428": error_response(),
                }
            }
        }),
    );
    add_aggregate_path(paths, ir, entity);
    lookup::add_path(paths, entity);
    super::bulk::add_paths(paths, ir, entity);
    add_workflow_paths(paths, entity);
    collections::add_paths(paths, ir, entity);
}

fn add_workflow_paths(paths: &mut Map<String, Value>, entity: &EntityIr) {
    let Some(workflow) = &entity.workflow else {
        return;
    };
    let singular = &entity.rust_name;
    let capabilities_path = format!("/api/{}/{{id}}/_transitions", entity.table_name);
    let transition_path = format!("{capabilities_path}/{{action}}");
    let mut base_parameters = vec![json!({
        "name": "id",
        "in": "path",
        "required": true,
        "schema": schema::primary_key(entity),
    })];
    if entity.tenant_scoped {
        base_parameters.push(tenant::parameter());
    }
    paths.insert(
        capabilities_path,
        json!({
            "get": {
                "operationId": format!("get{singular}WorkflowCapabilities"),
                "tags": [singular],
                "security": auth::security(&entity.access.read),
                "parameters": base_parameters,
                "responses": {
                    "200": versioned_response(
                        "Allowed workflow transitions for the current record revision",
                        &schema_ref(&format!("{singular}WorkflowCapabilities")),
                    ),
                    "404": error_response(),
                }
            }
        }),
    );
    let mut mutation_parameters = vec![
        json!({
            "name": "id",
            "in": "path",
            "required": true,
            "schema": schema::primary_key(entity),
        }),
        json!({
            "name": "action",
            "in": "path",
            "required": true,
            "schema": {
                "type": "string",
                "enum": workflow.transitions.iter().map(|transition| &transition.name).collect::<Vec<_>>(),
            },
        }),
        if_match_parameter(),
    ];
    if entity.tenant_scoped {
        mutation_parameters.push(tenant::parameter());
    }
    let access = AccessRuleIr::Any {
        rules: workflow
            .transitions
            .iter()
            .map(|transition| transition.access.clone())
            .collect(),
    };
    paths.insert(
        transition_path,
        json!({
            "post": {
                "operationId": format!("transition{singular}Workflow"),
                "tags": [singular],
                "security": auth::security(&access),
                "parameters": mutation_parameters,
                "requestBody": request_body(&format!("{singular}WorkflowTransitionInput")),
                "responses": {
                    "200": versioned_response("Workflow transition completed", &schema_ref(singular)),
                    "403": error_response(),
                    "404": error_response(),
                    "409": error_response(),
                    "412": error_response(),
                    "422": error_response(),
                    "428": error_response(),
                }
            }
        }),
    );
}
fn add_aggregate_path(paths: &mut Map<String, Value>, ir: &AppIr, entity: &EntityIr) {
    let singular = &entity.rust_name;
    let aggregate_path = format!("/api/{}/_aggregate", entity.table_name);
    paths.insert(
        aggregate_path,
        json!({
            "get": {
                "operationId": format!("aggregate{singular}"),
                "tags": [singular],
                "security": auth::security(&entity.access.list),
                "parameters": aggregate_parameters(ir, entity),
                "responses": {
                    "200": response(
                        "Aggregate and grouped resource data",
                        &schema_ref(&format!("{singular}AggregateResponse"))
                    ),
                    "400": error_response(),
                }
            }
        }),
    );
}
pub(super) fn add_schemas(schemas: &mut Map<String, Value>, entity: &EntityIr) {
    schema::add_schemas(schemas, entity);
}
fn list_parameters(ir: &AppIr, entity: &EntityIr) -> Vec<Value> {
    let mut parameters = vec![
        query_parameter(
            "page",
            &json!({ "type": "integer", "minimum": 1, "default": 1 }),
        ),
        query_parameter(
            "page_size",
            &json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }),
        ),
        query_parameter("cursor", &json!({ "type": "string" })),
        query_parameter(
            "limit",
            &json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }),
        ),
        query_parameter("sort", &json!({ "type": "string" })),
        query_parameter("q", &json!({ "type": "string" })),
    ];
    for field in entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
    {
        parameters.push(query_parameter(
            &format!("filter[{}]", field.rust_name),
            &schema::field_schema(field, false),
        ));
        if matches!(
            field.ty,
            FieldTypeIr::Integer
                | FieldTypeIr::Bigint
                | FieldTypeIr::Decimal
                | FieldTypeIr::Date
                | FieldTypeIr::Datetime
        ) {
            for operator in ["gte", "lte"] {
                parameters.push(query_parameter(
                    &format!("filter[{}][{operator}]", field.rust_name),
                    &schema::field_schema(field, false),
                ));
            }
        }
    }
    for relation_field in entity.fields.iter().filter(|field| {
        field.capabilities.filterable && matches!(field.ty, FieldTypeIr::Relation { .. })
    }) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        let target = ir
            .entities
            .iter()
            .find(|candidate| candidate.id == *target)
            .expect("validated relation target");
        for target_field in target
            .fields
            .iter()
            .filter(|field| field.capabilities.filterable)
        {
            parameters.push(query_parameter(
                &format!(
                    "filter[{}.{}]",
                    relation_field.api_name, target_field.rust_name
                ),
                &schema::field_schema(target_field, false),
            ));
            if matches!(
                target_field.ty,
                FieldTypeIr::Integer
                    | FieldTypeIr::Bigint
                    | FieldTypeIr::Decimal
                    | FieldTypeIr::Date
                    | FieldTypeIr::Datetime
            ) {
                for operator in ["gte", "lte"] {
                    parameters.push(query_parameter(
                        &format!(
                            "filter[{}.{}][{operator}]",
                            relation_field.api_name, target_field.rust_name
                        ),
                        &schema::field_schema(target_field, false),
                    ));
                }
            }
        }
    }
    parameters
}
fn aggregate_parameters(ir: &AppIr, entity: &EntityIr) -> Vec<Value> {
    let mut parameters = vec![
        query_parameter(
            "metrics",
            &json!({ "type": "string", "example": "count,sum:priority,avg:priority" }),
        ),
        query_parameter(
            "group_by",
            &json!({ "type": "string", "example": "status" }),
        ),
        query_parameter(
            "limit",
            &json!({ "type": "integer", "minimum": 1, "maximum": 500, "default": 100 }),
        ),
    ];
    parameters.extend(list_parameters(ir, entity).into_iter().filter(|parameter| {
        let name = parameter["name"].as_str().unwrap_or_default();
        name == "q" || name.starts_with("filter[")
    }));
    parameters
}
fn query_parameter(name: &str, schema: &Value) -> Value {
    json!({ "name": name, "in": "query", "required": false, "schema": schema })
}
