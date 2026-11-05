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

#[test]
fn search_indexes_are_typed_and_round_trip_into_migrations() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/m6-query-project");
    let ir = compile_project(&fixture).unwrap();
    let indexes = &ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Entry")
        .unwrap()
        .indexes;
    assert_eq!(
        indexes
            .iter()
            .filter(|index| index.method == appstruct_ir::IndexMethod::Gin)
            .count(),
        2
    );
    assert!(indexes.iter().any(|index| index.operator_class == Some(appstruct_ir::IndexOperatorClass::GinTrgmOps)));
    assert!(
        indexes
            .iter()
            .any(|index| index.operator_class == Some(appstruct_ir::IndexOperatorClass::JsonbOps))
    );
    let project = tempfile::tempdir().unwrap();
    fs::create_dir(project.path().join("spec")).unwrap();
    for name in ["appstruct.yaml", "spec/identity.yaml", "spec/entry.yaml"] {
        fs::copy(fixture.join(name), project.path().join(name)).unwrap();
    }
    let spec = fs::read_to_string(project.path().join("spec/entry.yaml")).unwrap();
    for invalid in [
        spec.replace("gin_trgm_ops", "jsonb_ops"),
        spec.replace("method: gin", "method: hash"),
        spec.replace("method: gin", "method: gin\n        unique: true"),
    ] {
        fs::write(project.path().join("spec/entry.yaml"), invalid).unwrap();
        assert!(compile_project(project.path()).is_err());
    }
}

mod tenant_sort {
    use appstruct_compiler::compile_project;
    use appstruct_ir::{AppIr, EntityIr, IndexIr};
    use std::{fs, path::Path};

    const SWITCH_OFF: &str = "  tenant:\n    enabled: true\n";
    const SWITCH_ON: &str = "  tenant:\n    enabled: true\n    sortable_indexes: true\n";

    fn project(switch: &str) -> tempfile::TempDir {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("spec")).unwrap();
        for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
            fs::copy(fixture.join(relative), temporary.path().join(relative)).unwrap();
        }
        replace(&temporary.path().join("appstruct.yaml"), SWITCH_OFF, switch);
        temporary
    }

    fn replace(path: &Path, old: &str, new: &str) {
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains(old), "fixture no longer contains {old:?}");
        fs::write(path, source.replacen(old, new, 1)).unwrap();
    }

    fn entity<'a>(ir: &'a AppIr, name: &str) -> &'a EntityIr {
        ir.entities
            .iter()
            .find(|entity| entity.rust_name == name)
            .unwrap()
    }

    fn tenant_sort(entity: &EntityIr) -> Vec<&IndexIr> {
        entity
            .indexes
            .iter()
            .filter(|index| index.id.contains("_tenant_sort_"))
            .collect()
    }

    /// The three column names an index covers, in index order.
    fn columns(entity: &EntityIr, index: &IndexIr) -> Vec<String> {
        index
            .fields
            .iter()
            .map(|id| {
                entity
                    .fields
                    .iter()
                    .find(|field| &field.id == id)
                    .unwrap()
                    .rust_name
                    .clone()
            })
            .collect()
    }

    fn error_codes(project: &Path) -> Vec<String> {
        compile_project(project)
            .unwrap_err()
            .into_iter()
            .map(|d| d.code.clone())
            .collect()
    }

    #[test]
    fn off_by_default_so_existing_migrations_do_not_change() {
        let ir = compile_project(project(SWITCH_OFF).path()).unwrap();
        assert!(
            ir.entities
                .iter()
                .all(|entity| tenant_sort(entity).is_empty())
        );
    }

    #[test]
    fn project_switch_indexes_every_sortable_field_with_tenant_first_and_key_last() {
        let ir = compile_project(project(SWITCH_ON).path()).unwrap();
        let projects = entity(&ir, "Project");
        let mut found = tenant_sort(projects)
            .into_iter()
            .map(|index| columns(projects, index))
            .collect::<Vec<_>>();
        found.sort();
        assert_eq!(
            found,
            [
                ["tenant_id", "created_at", "id"],
                ["tenant_id", "name", "id"]
            ]
        );
        assert!(
            tenant_sort(projects)
                .iter()
                .all(|index| !index.unique && index.predicate.is_none())
        );
        assert_eq!(
            tenant_sort(entity(&ir, "Task")).len(),
            1,
            "Task has one sortable field"
        );
        assert!(
            tenant_sort(entity(&ir, "User")).is_empty(),
            "User is not tenant-scoped"
        );
    }

    #[test]
    fn entity_can_opt_out_of_the_project_switch() {
        let temporary = project(SWITCH_ON);
        replace(
            &temporary.path().join("spec/project.yaml"),
            "    table: projects\n    tenant: true\n",
            "    table: projects\n    tenant: true\n    tenant_index: false\n",
        );
        let ir = compile_project(temporary.path()).unwrap();
        assert_eq!(tenant_sort(entity(&ir, "Project")).len(), 0);
        assert_eq!(tenant_sort(entity(&ir, "Task")).len(), 1);
    }

    #[test]
    fn entity_can_opt_in_while_the_project_switch_is_off() {
        let temporary = project(SWITCH_OFF);
        replace(
            &temporary.path().join("spec/project.yaml"),
            "    table: tasks\n    tenant: true\n",
            "    table: tasks\n    tenant: true\n    tenant_index: true\n",
        );
        let ir = compile_project(temporary.path()).unwrap();
        assert_eq!(tenant_sort(entity(&ir, "Task")).len(), 1);
        assert_eq!(tenant_sort(entity(&ir, "Project")).len(), 0);
    }

    #[test]
    fn opting_in_without_tenant_scope_is_an_error_not_a_silent_no_op() {
        let temporary = project(SWITCH_OFF);
        replace(
            &temporary.path().join("spec/identity.yaml"),
            "  User:\n",
            "  User:\n    tenant_index: true\n",
        );
        assert!(error_codes(temporary.path()).contains(&"AS2065".to_owned()));
    }

    #[test]
    fn project_switch_never_errors_for_entities_that_are_not_tenant_scoped() {
        // `User` is global; the switch is a default, not a request, so it must not fail.
        assert!(compile_project(project(SWITCH_ON).path()).is_ok());
    }

    #[test]
    fn soft_deleted_entities_get_a_partial_index_matching_the_read_filter() {
        let temporary = project(SWITCH_ON);
        replace(
            &temporary.path().join("spec/project.yaml"),
            "    table: projects\n    tenant: true\n",
            "    table: projects\n    tenant: true\n    soft_delete: true\n",
        );
        replace(
            &temporary.path().join("spec/project.yaml"),
            "      created_at:\n        type: datetime\n        generated: now\n        sortable: true\n",
            "      created_at:\n        type: datetime\n        generated: now\n        sortable: true\n      deleted_at:\n        type: datetime\n",
        );
        let ir = compile_project(temporary.path()).unwrap();
        let projects = entity(&ir, "Project");
        assert_eq!(tenant_sort(projects).len(), 2);
        assert!(
            tenant_sort(projects)
                .iter()
                .all(|index| index.predicate.as_deref() == Some("deleted_at IS NULL"))
        );
        assert!(
            tenant_sort(entity(&ir, "Task"))
                .iter()
                .all(|index| index.predicate.is_none())
        );
    }

    #[test]
    fn an_equivalent_hand_written_index_is_not_duplicated() {
        let temporary = project(SWITCH_ON);
        replace(
            &temporary.path().join("spec/project.yaml"),
            "    table: projects\n    tenant: true\n",
            "    table: projects\n    tenant: true\n    indexes:\n      - name: mine\n        fields: [tenant_id, name, id]\n",
        );
        let ir = compile_project(temporary.path()).unwrap();
        let projects = entity(&ir, "Project");
        let covering = projects
            .indexes
            .iter()
            .filter(|index| columns(projects, index) == ["tenant_id", "name", "id"])
            .count();
        assert_eq!(covering, 1, "the generated twin must be skipped");
        assert_eq!(
            tenant_sort(projects).len(),
            1,
            "created_at still gets its index"
        );
    }

    #[test]
    fn soft_delete_indexes_use_the_mapped_database_column() {
        for column in ["removed_at", "when"] {
            let temporary = project(SWITCH_ON);
            let spec = temporary.path().join("spec/project.yaml");
            replace(
                &spec,
                "    table: projects\n    tenant: true\n",
                "    table: projects\n    tenant: true\n    soft_delete: true\n",
            );
            replace(
                &spec,
                "    fields:\n",
                &format!(
                    "    fields:\n      deleted_at:\n        type: datetime\n        column: {column}\n"
                ),
            );
            let ir = compile_project(temporary.path()).unwrap();
            let expected = format!("\"{column}\" IS NULL");
            let indexes = tenant_sort(entity(&ir, "Project"));
            assert_eq!(indexes.len(), 2);
            assert!(
                indexes
                    .iter()
                    .all(|index| index.predicate.as_deref() == Some(expected.as_str()))
            );
        }
    }

    #[test]
    fn a_name_that_would_be_truncated_by_postgres_is_rejected() {
        let temporary = project(SWITCH_ON);
        replace(
            &temporary.path().join("spec/project.yaml"),
            "      name:\n",
            "      a_field_name_long_enough_to_push_the_index_name_past_the_limit:\n",
        );
        assert!(error_codes(temporary.path()).contains(&"AS2066".to_owned()));
    }

    #[test]
    fn a_short_declared_name_can_cover_a_long_sortable_field() {
        let temporary = project(SWITCH_ON);
        let long_name = "a_field_name_long_enough_to_push_the_index_name_past_the_limit";
        let spec = temporary.path().join("spec/project.yaml");
        replace(&spec, "      name:\n", &format!("      {long_name}:\n"));
        replace(
            &spec,
            "    table: projects\n    tenant: true\n",
            &format!(
                "    table: projects\n    tenant: true\n    indexes:\n      - name: short_name\n        fields: [tenant_id, {long_name}, id]\n"
            ),
        );
        let ir = compile_project(temporary.path()).unwrap();
        let projects = entity(&ir, "Project");
        assert_eq!(projects.indexes.len(), 2);
        assert_eq!(tenant_sort(projects).len(), 1);
    }

    #[test]
    fn an_equivalent_unique_index_already_covers_the_sort() {
        let temporary = project(SWITCH_ON);
        replace(
            &temporary.path().join("spec/project.yaml"),
            "    table: projects\n    tenant: true\n",
            "    table: projects\n    tenant: true\n    indexes:\n      - name: unique_sort\n        fields: [tenant_id, name, id]\n        unique: true\n",
        );
        let ir = compile_project(temporary.path()).unwrap();
        let projects = entity(&ir, "Project");
        assert_eq!(projects.indexes.len(), 2);
        assert_eq!(tenant_sort(projects).len(), 1);
        assert!(projects.indexes.iter().any(|index| index.unique));
    }

    #[test]
    fn output_is_deterministic() {
        let temporary = project(SWITCH_ON);
        let first = serde_json::to_string(&compile_project(temporary.path()).unwrap()).unwrap();
        let second = serde_json::to_string(&compile_project(temporary.path()).unwrap()).unwrap();
        assert_eq!(first, second);
    }
}
