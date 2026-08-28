use appstruct_compiler::compile_project;
use std::fs;

fn project_with_seed(seed: &str) -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: seed-demo\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        format!(
            "domain: core\nentities:\n  User:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n        generated: uuid_v7\n      email:\n        type: string\n        required: true\n      active:\n        type: boolean\n    seeds:\n{seed}\n    access:\n      list: {{ public: true }}\n      read: {{ public: true }}\n      create: {{ public: true }}\n      update: {{ public: true }}\n      delete: {{ public: true }}\n"
        ),
    )
    .unwrap();
    project
}

#[test]
fn lowers_named_seed_rows_with_typed_values() {
    let project = project_with_seed(
        "      admin:\n        id: 00000000-0000-0000-0000-000000000001\n        email: admin@example.com\n        active: true",
    );
    let ir = compile_project(project.path()).unwrap();
    assert_eq!(ir.seeds.len(), 1);
    assert_eq!(ir.seeds[0].id, "app::User::admin");
    assert_eq!(ir.seeds[0].values["active"], "true");
}

#[test]
fn seeds_require_primary_keys_and_valid_scalar_values() {
    let project =
        project_with_seed("      broken:\n        email: admin@example.com\n        active: yes");
    let diagnostics = compile_project(project.path()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS2062")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS2064")
    );
}
