use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use std::fs;
#[path = "support/mod.rs"]
#[allow(dead_code)]
mod support;

#[test]
fn soft_delete_resources_publish_trash_and_restore_contracts() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(project.path().join("appstruct.yaml"), "version: 1\napp:\n  name: trash-demo\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n").unwrap();
    fs::write(project.path().join("spec/domain.yaml"), "domain: core\nentities:\n  Note:\n    soft_delete: true\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n      title:\n        type: string\n      deleted_at:\n        type: datetime\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n").unwrap();
    let artifacts = plan(&compile_project(project.path()).unwrap()).unwrap();
    let api = artifacts
        .iter()
        .find(|artifact| artifact.relative_path.to_string_lossy() == "backend/src/api/note.rs")
        .unwrap();
    let source = String::from_utf8(api.content.clone()).unwrap();
    assert!(source.contains("/_restore"));
    assert!(source.contains("active.deleted_at = Set(Some"));
    assert!(source.contains("active.deleted_at = Set(None"));
    let openapi = artifacts
        .iter()
        .find(|artifact| artifact.relative_path.to_string_lossy() == "openapi/openapi.json")
        .unwrap();
    assert!(String::from_utf8_lossy(&openapi.content).contains("/api/notes/_restore"));
    for artifact in &artifacts {
        let destination = project
            .path()
            .join("generated")
            .join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &artifact.content).unwrap();
    }
    let backend = project.path().join("app/backend");
    fs::create_dir_all(backend.join("src")).unwrap();
    fs::write(backend.join("Cargo.toml"), "[package]\nname=\"appstruct-app-backend\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nappstruct-generated-backend={path=\"../../generated/backend\"}\n").unwrap();
    fs::write(backend.join("src/lib.rs"), "use appstruct_generated_backend::AppExtensions;\npub fn extensions() -> AppExtensions { AppExtensions::builder().build() }\n").unwrap();
    let checked = support::cargo_check(&project.path().join("generated/backend/Cargo.toml"), false);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}
