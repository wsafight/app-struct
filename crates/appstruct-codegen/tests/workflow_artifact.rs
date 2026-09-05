mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use serde_json::Value;
use std::fs;
use std::path::Path;
use support::{assert_rustfmt, cargo_check};

fn artifacts() -> Vec<Artifact> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-workflow-project");
    plan(&compile_project(&fixture).unwrap()).unwrap()
}

fn artifact_text<'a>(artifacts: &'a [Artifact], path: &str) -> &'a str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

#[test]
fn workflow_is_generated_as_a_dedicated_mutation_contract() {
    let artifacts = artifacts();
    let api = artifact_text(&artifacts, "backend/src/api/order.rs");
    let create_input = &api
        [api.find("pub struct CreateInput").unwrap()..api.find("pub struct UpdateInput").unwrap()];
    let update_input =
        &api[api.find("pub struct UpdateInput").unwrap()..api.find("pub fn router").unwrap()];
    assert!(!create_input.contains("status"));
    assert!(!update_input.contains("status"));
    assert!(api.contains("status: Set(\"draft\".to_owned())"));
    assert!(api.contains("/{id}/_transitions/{action}"));
    assert!(api.contains("lock_exclusive()"));
    assert!(api.contains("before.revision != expected"));
    assert!(api.contains("InvalidWorkflowState"));
    assert!(api.contains("before_transition"));
    assert!(api.contains("can_transition"));
    assert!(api.contains("workflow.{transition}"));
    assert!(api.contains("crate::webhooks::publish"));
    assert!(api.contains("publish_realtime_event"));
    assert!(api.contains("pub enum WorkflowTransitionInput"));
    let extensions = artifact_text(&artifacts, "backend/src/extensions.rs");
    assert!(extensions.contains("async fn can_view_transition"));
    let error = artifact_text(&artifacts, "backend/src/error.rs");
    assert!(error.contains("UNKNOWN_WORKFLOW_TRANSITION"));
    assert!(error.contains("INVALID_WORKFLOW_STATE"));
    assert!(error.contains("INVALID_WORKFLOW_INPUT"));
}

#[test]
fn workflow_is_published_to_openapi_typescript_and_web() {
    let artifacts = artifacts();
    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/orders/{id}/_transitions"]["get"].is_object());
    assert!(openapi["paths"]["/api/orders/{id}/_transitions/{action}"]["post"].is_object());
    assert_eq!(
        openapi["components"]["schemas"]["Order"]["properties"]["status"]["readOnly"],
        true,
    );
    assert!(openapi["components"]["schemas"]["CreateOrderInput"]["properties"]["status"].is_null());
    assert!(openapi["components"]["schemas"]["UpdateOrderInput"]["properties"]["status"].is_null());
    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("export type OrderWorkflowTransition"));
    assert!(client.contains("reject: (id: string, input: RejectOrderInput)"));
    assert!(client.contains("member, true"));
    let resources = artifact_text(&artifacts, "web/src/generated/resources.ts");
    assert!(resources.contains("workflow: { field: \"status\""));
    assert!(resources.contains("readOnly: true"));
    assert!(resources.contains("input: { name: \"RejectOrderInput\""));
    let detail = artifact_text(&artifacts, "web/src/pages/ResourceDetail.tsx");
    assert!(detail.contains("<WorkflowActions"));
    let actions = artifact_text(&artifacts, "web/src/pages/WorkflowActions.tsx");
    assert!(actions.contains("allowed_transitions"));
    assert!(actions.contains("resource.api.transition"));
}

#[test]
fn workflow_backend_is_rustfmt_clean_and_compiles() {
    let artifacts = artifacts();
    let temporary = tempfile::tempdir().unwrap();
    for artifact in artifacts {
        let destination = temporary
            .path()
            .join("generated")
            .join(artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, artifact.content).unwrap();
    }
    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, true);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr),
    );
}
