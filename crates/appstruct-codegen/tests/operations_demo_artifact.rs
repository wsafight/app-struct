mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check, cargo_check_with_features};

fn artifacts() -> Vec<Artifact> {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/operations-demo");
    plan(&compile_project(&demo).unwrap()).unwrap()
}

fn artifact_text<'a>(artifacts: &'a [Artifact], path: &str) -> &'a str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

#[test]
fn operations_demo_generation_is_deterministic_and_combined() {
    let first = artifacts();
    assert_eq!(first, artifacts());

    let runtime = artifact_text(&first, "backend/src/lib.rs");
    assert!(runtime.contains("report::router()"));
    assert!(runtime.contains("activity::router()"));
    let order = artifact_text(&first, "backend/src/api/order.rs");
    assert!(order.contains("workflow.{transition}"));
    assert!(order.contains("record_system_event"));
    assert!(order.contains("let access_condition = Condition::all()"));
    assert!(order.contains("Column::TenantId.eq(context.require_tenant()?)"));
    let sql = artifact_text(&first, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_jobs"));
    assert!(sql.contains("_appstruct_files"));
    assert!(sql.contains("_appstruct_report_runs"));
    assert!(sql.contains("_appstruct_activity_entries"));
    let manifest = artifact_text(&first, "backend/Cargo.toml");
    assert!(manifest.contains("test-support = []"));
    let jobs = artifact_text(&first, "backend/src/jobs.rs");
    assert!(jobs.contains("APPSTRUCT_TEST_JOB_GATE"));
    let report = artifact_text(&first, "backend/src/report.rs");
    assert!(report.contains("APPSTRUCT_TEST_REPORT_FAIL_ONCE"));
    let resources = artifact_text(&first, "web/src/generated/resources.ts");
    assert_eq!(resources.matches("kind: \"money\"").count(), 2);
    assert!(resources.contains("currencyField: \"currency\""));
    assert!(resources.contains("fractionDigits: 2"));
}

#[test]
fn generated_operations_backend_is_rustfmt_clean_and_compiles() {
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
    let checked = cargo_check_with_features(&manifest, true, &["test-support"]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr),
    );
}
