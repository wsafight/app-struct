mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check};

fn artifacts() -> Vec<Artifact> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-report-project");
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
fn report_backend_owns_authorization_idempotency_and_dispatch() {
    let artifacts = artifacts();
    let report = artifact_text(&artifacts, "backend/src/report.rs");
    assert!(report.contains("APPSTRUCT_REPORT_SNAPSHOT_KEY"));
    assert!(report.contains("jsonschema::validator_for"));
    assert!(report.contains("Idempotency-Key"));
    let error = artifact_text(&artifacts, "backend/src/error.rs");
    assert!(error.contains("REPORT_IDEMPOTENCY_CONFLICT"));
    assert!(report.contains("tenant_id IS NOT DISTINCT FROM"));
    assert!(report.contains("execution_job_id = $2"));
    assert!(report.contains("report.render"));
    assert!(report.contains("ReportJobPayload { run_id }"));
    assert!(!report.contains("bucket"));
    assert!(!report.contains("template_url"));
    let jobs = artifact_text(&artifacts, "backend/src/jobs.rs");
    let report_dispatch = jobs.find("\"report.render\" | \"report.cleanup\"").unwrap();
    let custom_dispatch = jobs.find("if let Some(custom)").unwrap();
    assert!(report_dispatch < custom_dispatch);
    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_report_templates"));
    assert!(sql.contains("_appstruct_report_runs"));
    assert!(
        sql.contains(
            "CHECK (\"status\" IN ('queued', 'running', 'succeeded', 'dead', 'cancelled'))"
        )
    );
    let openapi: serde_json::Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/reports/templates/{name}/runs"]["post"].is_object());
    assert!(openapi["paths"]["/api/reports/runs/{id}/download"]["get"].is_object());
    assert_eq!(
        openapi["components"]["schemas"]["ReportRun"]["properties"]["stage"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        6,
    );
    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("export interface ReportInputMap"));
    assert!(client.contains("\"order-summary\": { \"order_id\": string }"));
    assert!(client.contains("Idempotency-Key"));
    let app = artifact_text(&artifacts, "web/src/app/App.tsx");
    assert!(app.contains("path: \"/reports\""));
    let page = artifact_text(&artifacts, "web/src/report/ReportPage.tsx");
    assert!(page.contains("refetchInterval"));
    assert!(page.contains("reportApi.download"));
}

#[test]
fn generated_report_backend_is_rustfmt_clean_and_compiles() {
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
