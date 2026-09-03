use appstruct_compiler::compile_project;
use std::fs;

fn compile(source: &str) -> Result<appstruct_ir::AppIr, Vec<appstruct_ir::Diagnostic>> {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.yaml"), source).unwrap();
    compile_project(project.path())
}

fn codes(diagnostics: &[appstruct_ir::Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

#[test]
fn rejects_unsupported_root_version_name_and_provider() {
    let diagnostics =
        compile("version: 2\napp:\n  name: Demo\ndatabase:\n  provider: mysql\nincludes: []\n")
            .unwrap_err();
    let found = codes(&diagnostics);
    assert!(found.contains(&"AS1011"));
    assert!(found.contains(&"AS2001"));
    assert!(found.contains(&"AS4001"));
}

#[test]
fn rejects_duplicate_entities_and_tables() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: duplicates\ndatabase:\n  provider: postgres\nincludes:\n  - spec/a.yaml\n  - spec/b.yaml\n",
    )
    .unwrap();
    let entity = "domain: core\nentities:\n  Item:\n    table: items\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n";
    fs::write(project.path().join("spec/a.yaml"), entity).unwrap();
    fs::write(project.path().join("spec/b.yaml"), entity).unwrap();
    let diagnostics = compile_project(project.path()).unwrap_err();
    let found = codes(&diagnostics);
    assert!(found.contains(&"AS2002") || found.contains(&"AS2003"));
}

#[test]
fn rejects_invalid_entity_names_and_missing_primary_keys() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: names\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  bad-name:\n    fields:\n      title:\n        type: string\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();
    let diagnostics = compile_project(project.path()).unwrap_err();
    let found = codes(&diagnostics);
    assert!(found.contains(&"AS2001") || found.contains(&"AS2004"));
}
