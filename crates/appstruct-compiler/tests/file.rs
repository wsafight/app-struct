use appstruct_compiler::compile_project;
use appstruct_ir::FileProviderIr;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-file-project")
}

#[test]
fn lowers_file_provider_limits_and_sorted_content_types() {
    let file = compile_project(&fixture()).unwrap().file;
    assert!(file.enabled);
    assert_eq!(file.provider, FileProviderIr::Local);
    assert_eq!(file.local_root, ".appstruct/files");
    assert_eq!(file.max_bytes, 1024);
    assert_eq!(
        file.allowed_content_types,
        ["application/json", "image/png", "text/plain"]
    );
}

#[test]
fn file_rejects_provider_root_and_size_errors() {
    for (old, new, code) in [
        ("provider: local", "provider: disk", "AS3053"),
        (
            "local_root: .appstruct/files",
            "local_root: ../files",
            "AS3054",
        ),
        ("max_bytes: 1024", "max_bytes: 0", "AS3055"),
    ] {
        let temporary = copied_fixture();
        replace(&temporary.path().join("appstruct.yaml"), old, new);
        assert_diagnostic(temporary.path(), code);
    }
}

#[test]
fn file_requires_valid_content_types() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "allowed_content_types: [text/plain, image/png, application/json]",
        "allowed_content_types: []",
    );
    assert_diagnostic(temporary.path(), "AS3056");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "text/plain, image/png, application/json",
        "Text/Plain, image/png, application/json",
    );
    assert_diagnostic(temporary.path(), "AS3057");
}

#[test]
fn lowers_file_ui_semantic_and_requires_enabled_file_module() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/project.yaml"),
        "      created_at:\n        type: datetime\n        generated: now",
        "      attachment:\n        type: uuid\n        ui:\n          semantic: file\n      created_at:\n        type: datetime\n        generated: now",
    );
    let ir = compile_project(temporary.path()).unwrap();
    let project = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Project")
        .unwrap();
    let attachment = project
        .fields
        .iter()
        .find(|field| field.api_name == "attachment")
        .unwrap();
    assert_eq!(
        attachment.ui_semantic,
        Some(appstruct_ir::FieldSemanticIr::File)
    );

    replace(
        &temporary.path().join("appstruct.yaml"),
        "  file:\n    enabled: true",
        "  file:\n    enabled: false",
    );
    assert_diagnostic(temporary.path(), "AS2020");
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
