use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[test]
fn project_commands_emit_success_envelopes() {
    let project = copied_fixture("m2-project");

    let generated = run(project.path(), &["generate", "--format", "json"]);
    assert_success(&generated, "generate");
    let result = parse(&generated)["result"].clone();
    assert_eq!(result["mode"], "write");
    assert!(result["artifact_count"].as_u64().unwrap() > 0);

    let migration = run(project.path(), &["migrate", "plan", "--format", "json"]);
    assert_success(&migration, "migrate");
    assert_eq!(parse(&migration)["result"]["action"], "plan");

    let preset = copied_fixture("m6-preset-project");
    let shown = run(preset.path(), &["preset", "show", "--format", "json"]);
    assert_success(&shown, "preset");
    assert_eq!(parse(&shown)["result"]["name"], "appstruct/saas");
}

#[test]
fn new_emits_a_success_envelope_without_a_project() {
    let parent = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .current_dir(parent.path())
        .args([
            "new",
            "json-app",
            "--template",
            "minimal",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_success(&output, "new");
    assert_eq!(parse(&output)["result"]["template"], "minimal");
    assert!(parent.path().join("json-app/appstruct.yaml").is_file());
}

fn assert_success(output: &Output, command: &str) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = parse(output);
    assert_eq!(report["ok"], true);
    assert_eq!(report["result"]["command"], command);
}

fn parse(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn copied_fixture(name: &str) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    let temporary = tempfile::tempdir().unwrap();
    copy_directory(&source, temporary.path());
    temporary
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target: PathBuf = destination.join(entry.file_name());
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
        .env_remove("DATABASE_URL")
        .output()
        .unwrap()
}
