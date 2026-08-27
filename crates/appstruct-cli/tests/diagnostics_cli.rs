use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn generation_errors_honor_global_json_format_before_or_after_command() {
    let project = temporary_project("m2-project");
    assert!(run(&project, &["generate"]).status.success());
    fs::write(
        project.join("generated/openapi/openapi.json"),
        "manually changed\n",
    )
    .unwrap();

    for arguments in [
        vec!["--format", "json", "generate"],
        vec!["generate", "--format", "json"],
    ] {
        let output = run(&project, &arguments);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ok"], false);
        assert_eq!(report["error"]["code"], "AS5004");
        assert_eq!(report["error"]["category"], "generation");
        assert_eq!(report["error"]["exit_code"], 1);
    }
}

fn temporary_project(fixture: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(fixture);
    let destination = tempfile::tempdir().unwrap().keep();
    copy_directory(&source, &destination);
    destination
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

fn run(project: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(project)
        .args(arguments)
        .env_remove("DATABASE_URL")
        .output()
        .unwrap()
}
