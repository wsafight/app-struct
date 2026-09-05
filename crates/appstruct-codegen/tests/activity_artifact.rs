mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check};

fn artifacts() -> Vec<Artifact> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-activity-project");
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
fn activity_backend_owns_target_authorization_and_tombstones() {
    let artifacts = artifacts();
    let activity = artifact_text(&artifacts, "backend/src/activity.rs");
    assert!(activity.contains("authorize_activity_target"));
    assert!(activity.contains("tenant_id IS NOT DISTINCT FROM"));
    assert!(activity.contains("(e.occurred_at, e.id) <"));
    assert!(activity.contains("activity.comment.moderated"));
    assert!(activity.contains("governance_reason"));
    assert!(activity.contains("content_base64"));
    assert!(!activity.contains("input.object_key"));
    let order = artifact_text(&artifacts, "backend/src/api/order.rs");
    for event in ["created", "updated", "deleted", "restored"] {
        assert!(order.contains("record_system_event(\n"));
        assert!(order.contains(&format!("\"{event}\"")));
    }
    assert!(order.contains("workflow.{transition}"));
    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_activity_entries"));
    assert!(sql.contains("\"tenant_id\", \"resource\", \"record_id\", \"occurred_at\", \"id\""));
    let openapi: serde_json::Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/activity/{resource}/{record_id}"]["get"].is_object());
    assert!(
        openapi["paths"]["/api/activity/{resource}/{record_id}/{entry_id}/attachment"]["get"]
            .is_object()
    );
    assert_eq!(
        openapi["components"]["schemas"]["ActivityEntry"]["properties"]["kind"]["enum"],
        serde_json::json!(["comment", "system"]),
    );
    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("export const activityApi"));
    assert!(client.contains("export type ActivityResource = \"orders\""));
    let resources = artifact_text(&artifacts, "web/src/generated/resources.ts");
    assert!(resources.contains("activity: { maxCommentBytes: 4000, attachments: true"));
    let detail = artifact_text(&artifacts, "web/src/pages/ResourceDetail.tsx");
    assert!(detail.contains("<ActivityTimeline"));
    let realtime = artifact_text(&artifacts, "web/src/activity/useActivityRealtime.ts");
    assert!(realtime.contains("subscribeRealtime"));
    assert!(realtime.contains("activity.comment.created"));
    assert!(realtime.contains("workflow.${transition.name}"));
    assert!(realtime.contains("seen.has(eventId)"));
}

#[test]
fn generated_activity_backend_is_rustfmt_clean_and_compiles() {
    let temporary = tempfile::tempdir().unwrap();
    for artifact in artifacts() {
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
