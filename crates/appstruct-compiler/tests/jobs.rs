use appstruct_compiler::compile_project;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-jobs-project")
}

#[test]
fn lowers_jobs_settings_and_sorted_queues() {
    let jobs = compile_project(&fixture()).unwrap().jobs;
    assert!(jobs.enabled);
    assert_eq!(jobs.poll_interval_ms, 25);
    assert_eq!(jobs.lease_seconds, 2);
    assert_eq!(
        jobs.queues
            .iter()
            .map(|queue| queue.name.as_str())
            .collect::<Vec<_>>(),
        ["default", "mail"]
    );
    assert_eq!(jobs.queues[0].max_attempts, 2);
    assert_eq!(jobs.queues[1].backoff_seconds, 2);
    assert_eq!(jobs.schedules[0].name, "cleanup");
    assert_eq!(jobs.schedules[0].interval_seconds, 900);
    assert_eq!(jobs.schedules[0].kind, "maintenance.cleanup");
}

#[test]
fn jobs_rejects_invalid_schedules() {
    for (old, new, code) in [
        ("queue: default", "queue: missing", "AS3054"),
        ("cron: \"*/15 * * * *\"", "cron: yearly", "AS3057"),
        (
            "payload: '{\"scope\":\"expired\"}'",
            "payload: '{oops'",
            "AS3056",
        ),
    ] {
        let temporary = copied_fixture();
        replace(&temporary.path().join("appstruct.yaml"), old, new);
        assert_diagnostic(temporary.path(), code);
    }
}

#[test]
fn jobs_requires_queues_and_valid_ranges() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    queues:\n      default: { max_attempts: 2, backoff_seconds: 1 }\n      mail: { max_attempts: 3, backoff_seconds: 2 }",
        "    queues: {}",
    );
    assert_diagnostic(temporary.path(), "AS3047");

    for (old, new, code) in [
        ("poll_interval_ms: 25", "poll_interval_ms: 1", "AS3048"),
        ("lease_seconds: 2", "lease_seconds: 0", "AS3049"),
        ("max_attempts: 2", "max_attempts: 0", "AS3051"),
        ("backoff_seconds: 1", "backoff_seconds: 5000", "AS3052"),
    ] {
        let temporary = copied_fixture();
        replace(&temporary.path().join("appstruct.yaml"), old, new);
        assert_diagnostic(temporary.path(), code);
    }
}

#[test]
fn jobs_rejects_invalid_queue_names() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "      default: { max_attempts: 2, backoff_seconds: 1 }",
        "      Default: { max_attempts: 2, backoff_seconds: 1 }",
    );
    assert_diagnostic(temporary.path(), "AS3050");
}

fn copied_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(fixture().join(relative), temporary.path().join(relative)).unwrap();
    }
    temporary
}

fn replace(path: &Path, old: &str, new: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains(old));
    fs::write(path, source.replacen(old, new, 1)).unwrap();
}

fn assert_diagnostic(project: &Path, code: &str) {
    let diagnostics = compile_project(project).unwrap_err();
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "expected {code}, got {diagnostics:#?}"
    );
}
