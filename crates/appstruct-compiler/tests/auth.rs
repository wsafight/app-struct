use appstruct_compiler::compile_project;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

#[test]
fn rejects_access_roles_that_rbac_does_not_declare() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/identity.yaml"),
        "        role: member",
        "        role: auditor",
    );

    assert_diagnostic(temporary.path(), "AS3030");
}

#[test]
fn rejects_owner_fields_that_do_not_relate_to_the_auth_user() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/project.yaml"),
        "        target: User",
        "        target: Project",
    );

    assert_diagnostic(temporary.path(), "AS3033");
}

#[test]
fn rejects_empty_composite_access_rules() {
    for operator in ["any", "all"] {
        let temporary = copied_fixture();
        replace(
            &temporary.path().join("spec/project.yaml"),
            concat!(
                "      list:\n",
                "        any:\n",
                "          - owner: owner\n",
                "          - role: admin",
            ),
            &format!("      list:\n        {operator}: []"),
        );

        assert_diagnostic(temporary.path(), "AS1007");
    }
}

#[test]
fn rejects_incompatible_auth_user_identity_fields() {
    for (old, new) in [
        ("        type: uuid", "        type: integer"),
        ("        unique: true", "        unique: false"),
    ] {
        let temporary = copied_fixture();
        replace(&temporary.path().join("spec/identity.yaml"), old, new);

        assert_diagnostic(temporary.path(), "AS3028");
    }
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
