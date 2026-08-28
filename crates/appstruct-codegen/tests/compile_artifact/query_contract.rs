use super::artifact_text;
use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use serde_json::Value;
use std::path::Path;

#[test]
fn relation_filter_applies_target_access_and_tenant_scope() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let mut ir = compile_project(&fixture).unwrap();
    let project = ir
        .entities
        .iter_mut()
        .find(|entity| entity.rust_name == "Project")
        .unwrap();
    project
        .fields
        .iter_mut()
        .find(|field| field.rust_name == "name")
        .unwrap()
        .capabilities
        .filterable = true;
    let task = ir
        .entities
        .iter_mut()
        .find(|entity| entity.rust_name == "Task")
        .unwrap();
    task.fields
        .iter_mut()
        .find(|field| field.rust_name == "project_id")
        .unwrap()
        .capabilities
        .filterable = true;

    let artifacts = plan(&ir).unwrap();
    let api = artifact_text(&artifacts, "backend/src/api/task.rs");
    let related = &api[api.find("filter[project.name]").unwrap()..];
    assert!(related.contains("let relation_access_scope = context.actor()"));
    assert!(related.contains("project::Column::TenantId.eq(context.require_tenant()?)"));
    assert!(related.contains("project::Column::Name.eq(value)"));
}

pub(super) fn assert_query_contract(artifacts: &[Artifact]) {
    let client = artifact_text(artifacts, "web/src/generated/client.ts");
    assert!(client.contains("ListResponse"));
    assert!(client.contains("CursorListResponse"));
    assert!(client.contains("listCursor"));
    assert!(client.contains("{ limit: 25, ...query }"));
    assert!(client.contains("AggregateQuery"));
    assert!(client.contains("AggregateResponse"));
    assert!(client.contains("aggregatePath"));
    assert!(client.contains("aggregate: (query: AggregateQuery"));
    assert!(client.contains("range_filters"));

    let project_api = artifact_text(artifacts, "backend/src/api/project.rs");
    assert!(project_api.contains("cursor pagination cannot be combined"));
    assert!(project_api.contains("URL_SAFE_NO_PAD"));
    assert!(project_api.contains("limit + 1"));
    assert!(project_api.contains("ListMeta::Cursor"));
    let task_api = artifact_text(artifacts, "backend/src/api/task.rs");
    assert!(task_api.contains("filter[project.status]"));
    assert!(task_api.contains("project::Column::Status.eq(value)"));
    assert!(task_api.contains("in_subquery(relation_select.into_query())"));
    assert!(task_api.contains("async fn aggregate"));
    assert!(task_api.contains("Column::Priority.sum()"));
    assert!(task_api.contains("Column::Priority.avg()"));
    assert!(task_api.contains("Column::Priority.min()"));
    assert!(task_api.contains("Column::Priority.max()"));
    assert!(task_api.contains("group_priority"));
    assert!(task_api.contains("limit` must be between 1 and 500"));
    assert!(task_api.contains("aggregate metric `{metric}` is not allowed"));

    let openapi: Value =
        serde_json::from_str(artifact_text(artifacts, "openapi/openapi.json")).unwrap();
    let meta = &openapi["components"]["schemas"]["ProjectListResponse"]["properties"]["meta"];
    assert_eq!(meta["type"], "object");
    assert_eq!(meta["oneOf"].as_array().unwrap().len(), 2);
    let task_parameters = openapi["paths"]["/api/tasks/"]["get"]["parameters"]
        .as_array()
        .unwrap();
    for name in ["cursor", "limit", "filter[project.status]"] {
        assert!(
            task_parameters
                .iter()
                .any(|parameter| parameter["name"] == name),
            "missing query parameter {name}"
        );
    }
    assert!(
        !task_parameters
            .iter()
            .any(|parameter| parameter["name"] == "filter[project.name]")
    );
    assert!(openapi["paths"]["/api/tasks/_aggregate"]["get"].is_object());
    assert!(openapi["components"]["schemas"]["TaskAggregateResponse"].is_object());
    assert!(
        openapi["paths"]["/api/tasks/_aggregate"]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|parameter| parameter["name"] == "metrics")
    );
}
