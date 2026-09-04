mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use serde_json::Value;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check};

#[test]
fn audit_contract_generates_a_compilable_backend() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-audit-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_audit_events"));
    assert!(sql.contains("CHECK (\"operation\" IN ('create', 'update', 'delete', 'restore'))"));
    assert!(sql.contains("FOREIGN KEY (\"actor_id\")"));
    assert!(sql.contains("FOREIGN KEY (\"tenant_id\")"));

    let api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(api.contains("crate::audit::record"));
    assert!(api.contains("\"update\""));
    assert!(api.contains("Some(&before)"));
    assert!(api.contains("Some(&after)"));
    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("export const auditApi"));
    let audit_page = artifact_text(&artifacts, "web/src/audit/AuditPage.tsx");
    assert!(audit_page.contains("diffSnapshots"));
    assert!(audit_page.contains("Changed fields"));
    assert!(
        artifacts
            .iter()
            .any(|artifact| { artifact.relative_path == Path::new("web/src/audit/AuditPage.tsx") })
    );

    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/audit/events"]["get"].is_object());
    assert_eq!(
        openapi["paths"]["/api/audit/events"]["get"]["parameters"][2]["name"],
        "X-AppStruct-Tenant"
    );
    assert_eq!(
        openapi["components"]["schemas"]["AuditEvent"]["properties"]["operation"]["enum"],
        serde_json::json!(["create", "update", "delete", "restore"])
    );
    assert!(client.contains("operation: \"create\" | \"update\" | \"delete\" | \"restore\""));

    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, true);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

fn write_artifacts(root: &Path, artifacts: &[Artifact]) {
    for artifact in artifacts {
        let destination = root.join("generated").join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &artifact.content).unwrap();
    }
}
