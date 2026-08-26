use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::{fs, path::Path, process::Command};

#[test]
fn jobs_contract_generates_a_compilable_backend() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-jobs-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_jobs"));
    assert!(sql.contains("\"idempotency_key\" TEXT UNIQUE"));
    assert!(sql.contains("FOREIGN KEY (\"tenant_id\")"));
    let jobs = artifact_text(&artifacts, "backend/src/jobs.rs");
    assert!(jobs.contains("FOR UPDATE SKIP LOCKED"));
    assert!(jobs.contains("status = 'running' AND locked_until <= CURRENT_TIMESTAMP"));
    assert!(jobs.contains("pub struct JobWorkerHandle"));
    assert!(jobs.contains("pub async fn shutdown"));
    assert!(jobs.contains("pub fn for_kind"));
    assert!(jobs.contains("pub struct MailJobPayload"));
    assert!(jobs.contains("impl JobHandler for MailJobHandler"));
    let extensions = artifact_text(&artifacts, "backend/src/extensions.rs");
    assert!(extensions.contains("pub async fn enqueue_job"));
    assert!(extensions.contains("pub fn job_handler"));
    let library = artifact_text(&artifacts, "backend/src/lib.rs");
    assert!(library.contains("pub struct Application"));
    assert!(library.contains("shutdown signal received"));
    assert!(!library.contains("expect(\"invalid AppStruct"));
    let main = artifact_text(&artifacts, "backend/src/main.rs");
    assert!(main.contains("Application::from_env"));
    assert!(main.contains("application.serve(listener)"));

    let checked = cargo_check(&temporary.path().join("generated/backend/Cargo.toml"));
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

fn cargo_check(manifest: &Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "--quiet", "--manifest-path"])
        .arg(manifest)
        .env(
            "CARGO_TARGET_DIR",
            manifest.parent().unwrap().join("target"),
        )
        .output()
        .unwrap()
}
