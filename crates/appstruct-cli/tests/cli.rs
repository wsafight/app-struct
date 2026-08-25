use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[test]
fn check_emits_machine_readable_diagnostics_contract() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .args([
            "--project",
            fixture.to_str().unwrap(),
            "check",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], true);
    assert_eq!(report["entity_count"], 2);
    assert_eq!(report["diagnostics"], serde_json::json!([]));
}

#[test]
fn project_discovery_failure_honors_json_format() {
    let temporary = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .args([
            "--project",
            temporary.path().to_str().unwrap(),
            "check",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["diagnostics"][0]["code"], "AS1008");
}
