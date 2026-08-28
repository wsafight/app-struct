use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn migration_lint_reports_plan_risks_in_json() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(project.path().join("appstruct.yaml"), "version: 1\napp:\n  name: lint-demo\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n").unwrap();
    fs::write(project.path().join("spec/domain.yaml"), "domain: core\nentities:\n  User:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n      email:\n        type: string\n        required: true\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_appstruct"))
            .args(["--project", project.path().to_str().unwrap()])
            .args(args)
            .output()
            .unwrap()
    };
    let initial = run(&["migrate", "lint", "--format", "json"]);
    assert!(initial.status.success());
    let report: Value = serde_json::from_slice(&initial.stdout).unwrap();
    assert_eq!(report["result"]["valid"], true);
    assert!(run(&["migrate", "dev", "--accept"]).status.success());
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities: {}\n",
    )
    .unwrap();
    let lint = run(&["migrate", "lint", "--format", "json"]);
    assert_eq!(lint.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&lint.stdout).unwrap();
    assert_eq!(report["result"]["valid"], false);
    assert!(
        report["result"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "AS4201")
    );
}
