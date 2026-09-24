use serde_json::Value;
use std::process::Command;

#[test]
fn capabilities_reports_social_login_and_billing_status_without_a_project() {
    let temporary = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .current_dir(temporary.path())
        .args(["--format", "json", "capabilities"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["result"]["auth"]["providers"][1]["id"], "google");
    assert_eq!(report["result"]["auth"]["providers"][2]["id"], "github");
    assert_eq!(
        report["result"]["billing"]["providers"][0]["status"],
        "supported"
    );
}
