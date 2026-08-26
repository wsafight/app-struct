use appstruct_compiler::compile_project;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-audit-project")
}

#[test]
fn lowers_audit_module_and_entity_flag() {
    let ir = compile_project(&fixture()).unwrap();
    assert!(ir.audit.enabled);
    assert_eq!(ir.audit.reader_roles, ["admin"]);
    assert!(
        ir.entities
            .iter()
            .find(|entity| entity.rust_name == "Project")
            .unwrap()
            .audit_enabled
    );
}

#[test]
fn audit_requires_auth_and_reader_roles() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    enabled: true\n    user_entity: User",
        "    enabled: false\n    user_entity: User",
    );
    assert_diagnostic(temporary.path(), "AS3037");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    reader_roles: [admin]",
        "    reader_roles: []",
    );
    assert_diagnostic(temporary.path(), "AS3038");
}

#[test]
fn audit_rejects_unknown_reader_role_and_disabled_module() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    reader_roles: [admin]",
        "    reader_roles: [auditor]",
    );
    assert_diagnostic(temporary.path(), "AS3039");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "  audit:\n    enabled: true",
        "  audit:\n    enabled: false",
    );
    assert_diagnostic(temporary.path(), "AS3040");
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
