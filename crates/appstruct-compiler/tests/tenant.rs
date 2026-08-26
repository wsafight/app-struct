use appstruct_compiler::compile_project;
use appstruct_ir::GeneratedValueIr;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project")
}

#[test]
fn lowers_tenant_module_and_injected_field() {
    let ir = compile_project(&fixture()).unwrap();
    let project = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Project")
        .unwrap();
    assert!(ir.tenant.enabled);
    assert!(project.tenant_scoped);
    assert!(project.fields.iter().any(|field| {
        field.rust_name == "tenant_id" && matches!(field.generated, Some(GeneratedValueIr::Tenant))
    }));
}

#[test]
fn tenant_module_requires_auth() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    enabled: true\n    user_entity: User",
        "    enabled: false\n    user_entity: User",
    );
    assert_diagnostic(temporary.path(), "AS3034");
}

#[test]
fn tenant_scoped_entity_requires_module() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "  tenant:\n    enabled: true",
        "  tenant:\n    enabled: false",
    );
    assert_diagnostic(temporary.path(), "AS3035");
}

#[test]
fn tenant_id_is_reserved() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/project.yaml"),
        "      id:\n",
        "      tenant_id:\n        type: uuid\n        required: true\n      id:\n",
    );
    assert_diagnostic(temporary.path(), "AS3036");
}

#[test]
fn global_entity_cannot_reference_tenant_entity() {
    let temporary = copied_fixture();
    let identity = temporary.path().join("spec/identity.yaml");
    replace(
        &identity,
        "      email:\n",
        "      project:\n        type: relation\n        target: Project\n      email:\n",
    );
    assert_diagnostic(temporary.path(), "AS3037");
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
