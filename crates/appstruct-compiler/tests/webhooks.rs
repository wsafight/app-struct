use appstruct_compiler::compile_project;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-jobs-project")
}

#[test]
fn lowers_signed_webhook_endpoints() {
    let webhooks = compile_project(&fixture()).unwrap().webhooks;
    assert!(webhooks.enabled);
    assert_eq!(webhooks.poll_interval_ms, 50);
    assert_eq!(webhooks.connect_timeout_ms, 200);
    assert_eq!(webhooks.read_timeout_ms, 300);
    assert_eq!(webhooks.request_timeout_ms, 500);
    let operations = webhooks
        .endpoints
        .iter()
        .find(|endpoint| endpoint.name == "operations")
        .unwrap();
    assert_eq!(operations.max_attempts, 4);
    assert_eq!(operations.events, ["project.created"]);
}

#[test]
fn rejects_insecure_or_incomplete_webhook_endpoints() {
    for (old, new, code) in [
        (
            "http://127.0.0.1:__WEBHOOK_PORT__/ok",
            "http://example.com/hook",
            "AS3074",
        ),
        (
            "APPSTRUCT_WEBHOOK_OPERATIONS_SECRET",
            "secret-name",
            "AS3075",
        ),
        ("[project.created]", "[]", "AS3076"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        copy_project(&fixture(), temporary.path());
        let path = temporary.path().join("appstruct.yaml");
        let source = fs::read_to_string(&path).unwrap();
        fs::write(path, source.replacen(old, new, 1)).unwrap();
        let diagnostics = compile_project(temporary.path()).unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == code));
    }
}

#[test]
fn rejects_unbounded_webhook_timeouts() {
    let temporary = tempfile::tempdir().unwrap();
    copy_project(&fixture(), temporary.path());
    let path = temporary.path().join("appstruct.yaml");
    let source = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        source.replacen("request_timeout_ms: 500", "request_timeout_ms: 30000", 1),
    )
    .unwrap();
    let diagnostics = compile_project(temporary.path()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS3086")
    );
}

fn copy_project(source: &Path, destination: &Path) {
    fs::create_dir(destination.join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), destination.join(relative)).unwrap();
    }
}
