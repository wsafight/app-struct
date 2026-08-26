use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const DIGEST: &str = "sha256:7267c3a362b328d5b162f4536ac7115c72ab81cf9f3e17542a8d9b6eba7965c5";

#[test]
fn preset_show_reports_lock_and_expanded_defaults() {
    let project = fixture("m6-preset-project");
    let summary = run(&project, &["preset", "show"]);
    assert!(summary.status.success());
    let stdout = String::from_utf8_lossy(&summary.stdout);
    assert!(stdout.contains("appstruct/saas 1"));
    assert!(stdout.contains(&format!("digest: {DIGEST}")));
    assert!(stdout.contains("modules: audit, auth, file, jobs, mail, rbac, tenant"));

    let expanded = run(&project, &["preset", "show", "--expanded"]);
    assert!(expanded.status.success());
    let stdout = String::from_utf8_lossy(&expanded.stdout);
    assert!(stdout.starts_with("modules:\n  audit:\n"));
    assert!(stdout.contains("provider: capture"));
    assert!(stdout.contains("allowed_content_types:"));
}

#[test]
fn preset_show_expanded_includes_project_overrides() {
    let project = copied_fixture();
    let path = project.path().join("appstruct.yaml");
    let source = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        source.replacen(
            "includes:\n",
            "modules:\n  auth:\n    registration: false\n  jobs:\n    poll_interval_ms: 750\n\nincludes:\n",
            1,
        ),
    )
    .unwrap();

    let expanded = run(project.path(), &["preset", "show", "--expanded"]);
    assert!(expanded.status.success());
    let stdout = String::from_utf8_lossy(&expanded.stdout);
    assert!(stdout.contains("registration: false"));
    assert!(stdout.contains("password_reset: true"));
    assert!(stdout.contains("poll_interval_ms: 750"));
}

#[test]
fn preset_show_rejects_missing_and_tampered_locks() {
    let missing = copied_fixture();
    fs::remove_file(missing.path().join("appstruct.lock")).unwrap();
    assert_error(&run(missing.path(), &["preset", "show"]), "AS3059");

    let tampered = copied_fixture();
    let lock_path = tampered.path().join("appstruct.lock");
    let lock = fs::read_to_string(&lock_path).unwrap();
    fs::write(&lock_path, lock.replace(DIGEST, "sha256:0000")).unwrap();
    assert_error(&run(tampered.path(), &["preset", "show"]), "AS3060");
}

#[test]
fn preset_show_rejects_projects_without_a_preset() {
    assert_error(&run(&fixture("m0-project"), &["preset", "show"]), "AS6007");
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn copied_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m6-preset-project"), temporary.path());
    temporary
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn run(project: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(project)
        .args(arguments)
        .output()
        .unwrap()
}

fn assert_error(output: &Output, code: &str) {
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(code));
}
