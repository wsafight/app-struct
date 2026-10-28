use std::{fs, path::PathBuf, process::Command};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

fn preview(args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(fixture())
        .args(["access", "preview", "--format", "json"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn anonymous_role_and_owner_explanations_preserve_runtime_boundary_and_omit_sample_values() {
    let anonymous = preview(&["--entity", "Project"]);
    assert_eq!(anonymous["result"]["scope"], "declarations_only");
    let rows = anonymous["result"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["declarative"]["decision"], "denied");
    let admin = preview(&["--entity", "Project", "--role", "admin"]);
    assert_eq!(
        admin["result"]["rows"][0]["declarative"]["decision"],
        "allowed"
    );
    let sample = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        sample.path(),
        r#"{"owner":"00000000-0000-4000-8000-000000000001","name":"private sample marker"}"#,
    )
    .unwrap();
    let owner = preview(&[
        "--entity",
        "Project",
        "--actor-id",
        "00000000-0000-4000-8000-000000000001",
        "--record",
        sample.path().to_str().unwrap(),
    ]);
    assert_eq!(
        owner["result"]["rows"][0]["declarative"]["decision"],
        "allowed"
    );
    assert!(
        owner["result"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["runtime_policy"] == "not_evaluated")
    );
    assert!(!owner.to_string().contains("private sample marker"));
    let partial = preview(&["--entity", "Project", "--authenticated"]);
    assert_eq!(
        partial["result"]["rows"][0]["declarative"]["decision"],
        "requires_record"
    );
}

#[test]
fn unknown_roles_and_invalid_record_shapes_fail_without_database_access() {
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(fixture())
        .args(["access", "preview", "--role", "unknown"])
        .env_remove("DATABASE_URL")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let sample = tempfile::NamedTempFile::new().unwrap();
    fs::write(sample.path(), "[]").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(fixture())
        .args([
            "access",
            "preview",
            "--entity",
            "Project",
            "--record",
            sample.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
