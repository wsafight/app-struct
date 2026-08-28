use appstruct_compiler::compile_project;
use std::fs;

#[test]
fn lowers_composite_and_partial_indexes_deterministically() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: index-demo\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  User:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n      organization_id:\n        type: uuid\n      email:\n        type: string\n      deleted_at:\n        type: datetime\n    indexes:\n      - fields: [organization_id, email]\n      - name: active_user_email\n        fields: [email]\n        unique: true\n        where: deleted_at IS NULL\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();

    let ir = compile_project(project.path()).unwrap();
    let indexes = &ir.entities[0].indexes;
    assert_eq!(indexes.len(), 2);
    let mut lengths = indexes
        .iter()
        .map(|index| index.fields.len())
        .collect::<Vec<_>>();
    lengths.sort_unstable();
    assert_eq!(lengths, [1, 2]);
    let partial = indexes
        .iter()
        .find(|index| index.unique)
        .expect("partial unique index");
    assert_eq!(partial.predicate.as_deref(), Some("deleted_at IS NULL"));
}

#[test]
fn rejects_unsafe_partial_index_predicates() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: index-demo\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  User:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n      email:\n        type: string\n    indexes:\n      - fields: [email]\n        where: email IS NOT NULL; DROP TABLE users\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();

    let diagnostics = compile_project(project.path()).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS1013");
}
