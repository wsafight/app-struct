use appstruct_compiler::compile_project;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-activity-project")
}

#[test]
fn lowers_activity_contract_and_official_module() {
    let ir = compile_project(&fixture()).unwrap();
    assert!(ir.activity.enabled);
    assert_eq!(ir.activity.max_comment_bytes, 4_000);
    assert!(ir.activity.attachments);
    assert_eq!(ir.activity.admin_roles, ["admin"]);
    assert_eq!(ir.activity.resources.len(), 1);
    assert_eq!(ir.activity.resources[0].entity.0, "app::Order");
    assert_eq!(ir.activity.resources[0].resource, "orders");
    let module = ir
        .modules
        .iter()
        .find(|module| module.name == "appstruct/activity")
        .unwrap();
    assert_eq!(module.provides, ["activity.timeline"]);
    assert_eq!(
        module.requires,
        ["audit.events", "auth.identity", "file.storage"]
    );
}

#[test]
fn rejects_missing_dependencies_unknown_resources_and_roles() {
    let cases = [
        (
            "  audit:\n    enabled: true",
            "  audit:\n    enabled: false",
            "AS3100",
        ),
        (
            "    resources: [Order]",
            "    resources: [Missing]",
            "AS3103",
        ),
        (
            "    admin_roles: [admin]",
            "    admin_roles: [moderator]",
            "AS3104",
        ),
    ];
    for (old, new, code) in cases {
        let temporary = copy_fixture();
        replace(&temporary.path().join("appstruct.yaml"), old, new);
        let diagnostics = compile_project(temporary.path()).unwrap_err();
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "expected {code}, got {diagnostics:#?}",
        );
    }
}

fn copy_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/order.yaml"] {
        fs::copy(fixture().join(relative), temporary.path().join(relative)).unwrap();
    }
    temporary
}

fn replace(path: &Path, old: &str, new: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains(old), "fixture does not contain {old}");
    fs::write(path, source.replacen(old, new, 1)).unwrap();
}
