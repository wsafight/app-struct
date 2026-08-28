use appstruct_compiler::compile_project;
use std::fs;

fn project(soft_delete: bool) -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: trash-demo\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        format!(
            "domain: core\nentities:\n  Note:\n    soft_delete: {soft_delete}\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n      title:\n        type: string\n      deleted_at:\n        type: datetime\n    access:\n      list: {{ public: true }}\n      read: {{ public: true }}\n      create: {{ public: true }}\n      update: {{ public: true }}\n      delete: {{ public: true }}\n"
        ),
    )
    .unwrap();
    project
}

#[test]
fn soft_delete_requires_a_nullable_deleted_at_field() {
    let ir = compile_project(project(true).path()).unwrap();
    assert!(ir.entities[0].views.soft_delete);
}

#[test]
fn soft_delete_reports_missing_deleted_at() {
    let project = project(true);
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  Note:\n    soft_delete: true\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();
    let diagnostics = compile_project(project.path()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS2041")
    );
}
