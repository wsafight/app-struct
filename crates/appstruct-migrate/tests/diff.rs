use appstruct_compiler::compile_project;
use appstruct_migrate::{
    ColumnSchema, DatabaseType, ExecutionRisk, IndexSchema, SchemaChange, SchemaRisk, SeedSchema,
    SeedValueSchema, diff, extract, from_json, initial_migration, migration_sql,
};
use std::path::Path;

fn fixture_schema() -> appstruct_migrate::DatabaseSchema {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    extract(&compile_project(&fixture).unwrap()).unwrap()
}

fn tenant_fixture_schema() -> appstruct_migrate::DatabaseSchema {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    extract(&compile_project(&fixture).unwrap()).unwrap()
}

#[test]
fn nullable_column_addition_is_automatic() {
    let before = fixture_schema();
    let mut after = before.clone();
    after.tables[0].columns.push(ColumnSchema {
        id: "app::Project.notes".to_owned(),
        name: "notes".to_owned(),
        data_type: DatabaseType::Text,
        nullable: true,
        primary_key: false,
        unique: false,
        default: None,
        generated: None,
    });
    let plan = diff(&before, &after);
    assert_eq!(plan.changes.len(), 1);
    assert!(!plan.is_blocked());
    assert_eq!(plan.changes[0].risk.schema, SchemaRisk::NonDestructive);
    assert_eq!(plan.changes[0].risk.execution, ExecutionRisk::Online);
}

#[test]
fn removing_a_column_or_table_is_blocked() {
    let before = fixture_schema();
    let mut without_column = before.clone();
    without_column.tables[0].columns.remove(0);
    let column_plan = diff(&before, &without_column);
    assert!(column_plan.is_blocked());
    assert!(matches!(
        column_plan.changes[0].change,
        SchemaChange::RemoveColumn { .. }
    ));

    let mut without_table = before.clone();
    without_table.tables.remove(0);
    let table_plan = diff(&before, &without_table);
    assert!(table_plan.is_blocked());
    assert!(
        table_plan
            .changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveTable { .. }))
    );
}

#[test]
fn initial_sql_contains_unique_enum_and_relation_constraints() {
    let sql = initial_migration(&fixture_schema());
    assert!(sql.contains("\"code\" TEXT NOT NULL UNIQUE"));
    assert!(sql.contains("\"revision\" BIGINT NOT NULL DEFAULT 1"));
    assert!(sql.contains("CHECK (\"status\" IN ('planned', 'active', 'completed'))"));
    assert!(sql.contains("FOREIGN KEY (\"project_id\")"));
    assert!(sql.contains("ON DELETE CASCADE"));
}

#[test]
fn tenant_relations_use_composite_database_constraints() {
    let schema = tenant_fixture_schema();
    let relation = schema
        .foreign_keys
        .iter()
        .find(|foreign_key| foreign_key.source_table == "tasks")
        .unwrap();
    assert_eq!(relation.source_columns, ["tenant_id", "project_id"]);
    assert_eq!(relation.target_columns, ["tenant_id", "id"]);
    assert!(schema.unique_constraints.iter().any(|constraint| {
        constraint.table == "projects" && constraint.columns == ["tenant_id", "id"]
    }));

    let sql = initial_migration(&schema);
    assert!(sql.contains("UNIQUE (\"tenant_id\", \"id\")"));
    assert!(sql.contains(
        "FOREIGN KEY (\"tenant_id\", \"project_id\") REFERENCES \"projects\" (\"tenant_id\", \"id\")"
    ));
}

#[test]
fn legacy_single_column_foreign_keys_remain_readable() {
    let mut value = serde_json::to_value(fixture_schema()).unwrap();
    value["schema_version"] = 1.into();
    value.as_object_mut().unwrap().remove("unique_constraints");
    for foreign_key in value["foreign_keys"].as_array_mut().unwrap() {
        let object = foreign_key.as_object_mut().unwrap();
        for (old, new) in [
            ("source_columns", "source_column"),
            ("target_columns", "target_column"),
        ] {
            let column = object.remove(old).unwrap().as_array().unwrap()[0].clone();
            object.insert(new.to_owned(), column);
        }
    }

    let restored = from_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(restored.schema_version, 2);
    assert!(restored.unique_constraints.is_empty());
    assert_eq!(restored.foreign_keys[0].source_columns, ["project_id"]);
}

#[test]
fn table_rename_and_column_shape_changes_are_blocked() {
    let before = fixture_schema();

    let mut renamed = before.clone();
    renamed.tables[0].name = "renamed_projects".to_owned();
    let rename_plan = diff(&before, &renamed);
    assert!(rename_plan.is_blocked());
    assert!(matches!(
        rename_plan.changes[0].change,
        SchemaChange::RenameTable { .. }
    ));

    let mut changed_type = before.clone();
    changed_type.tables[1]
        .columns
        .iter_mut()
        .find(|column| column.name == "priority")
        .unwrap()
        .data_type = DatabaseType::Bigint;
    let type_plan = diff(&before, &changed_type);
    assert!(type_plan.is_blocked());
    assert_eq!(type_plan.changes[0].risk.schema, SchemaRisk::Destructive);

    let mut changed_key = before.clone();
    changed_key.tables[0]
        .columns
        .iter_mut()
        .find(|column| column.name == "id")
        .unwrap()
        .primary_key = false;
    let key_plan = diff(&before, &changed_key);
    assert!(key_plan.is_blocked());
    assert_eq!(key_plan.changes[0].risk.schema, SchemaRisk::Destructive);
}

#[test]
fn adding_a_foreign_key_to_existing_tables_requires_review() {
    let after = fixture_schema();
    let mut before = after.clone();
    before.foreign_keys.clear();

    let plan = diff(&before, &after);
    assert!(plan.is_blocked());
    assert_eq!(plan.changes[0].risk.schema, SchemaRisk::NonDestructive);
    assert_eq!(plan.changes[0].risk.execution, ExecutionRisk::MayLock);
}

#[test]
fn default_change_and_nullable_relaxation_render_online_sql() {
    let before = fixture_schema();
    let mut after = before.clone();
    let priority = after.tables[1]
        .columns
        .iter_mut()
        .find(|column| column.name == "priority")
        .unwrap();
    priority.nullable = true;
    priority.default = Some("1".to_owned());

    let plan = diff(&before, &after);
    assert!(!plan.is_blocked());
    let sql = migration_sql(&plan).unwrap();
    assert!(sql.contains("ALTER COLUMN \"priority\" DROP NOT NULL"));
    assert!(sql.contains("ALTER COLUMN \"priority\" SET DEFAULT 1"));
}

#[test]
fn composite_and_partial_indexes_are_diffed_and_rendered() {
    let before = fixture_schema();
    let mut after = before.clone();
    after.indexes.push(IndexSchema {
        id: "app::Project::active_name".to_owned(),
        table: "projects".to_owned(),
        columns: vec!["name".to_owned(), "status".to_owned()],
        unique: true,
        predicate: Some("status = 'active'".to_owned()),
    });
    let plan = diff(&before, &after);
    assert_eq!(plan.changes.len(), 1);
    assert!(matches!(
        plan.changes[0].change,
        SchemaChange::AddIndex { .. }
    ));
    assert_eq!(plan.changes[0].risk.execution, ExecutionRisk::MayLock);
    assert!(plan.is_blocked());

    let initial = initial_migration(&after);
    assert!(initial.contains(
        "CREATE UNIQUE INDEX \"idx_active_name\" ON \"projects\" (\"name\", \"status\") WHERE (status = 'active');"
    ));
}

#[test]
fn seed_rows_are_diffed_and_rendered_idempotently() {
    let before = fixture_schema();
    let mut after = before.clone();
    after.seeds.push(SeedSchema {
        id: "app::Project::demo".to_owned(),
        table: "projects".to_owned(),
        values: vec![
            SeedValueSchema {
                column: "id".to_owned(),
                value: "00000000-0000-0000-0000-000000000001".to_owned(),
                data_type: DatabaseType::Uuid,
            },
            SeedValueSchema {
                column: "priority".to_owned(),
                value: "3".to_owned(),
                data_type: DatabaseType::Integer,
            },
        ],
    });
    let plan = diff(&before, &after);
    assert!(matches!(
        plan.changes[0].change,
        SchemaChange::AddSeed { .. }
    ));
    assert!(!plan.is_blocked());
    let sql = migration_sql(&plan).unwrap();
    assert!(sql.contains("INSERT INTO \"projects\" (\"id\", \"priority\") VALUES ('00000000-0000-0000-0000-000000000001', 3) ON CONFLICT DO NOTHING;"));
}

#[test]
fn index_and_seed_replacements_are_diffed() {
    let after = fixture_schema();
    let mut before = after.clone();
    before.indexes.push(IndexSchema {
        id: "app::Project::legacy".to_owned(),
        table: "projects".to_owned(),
        columns: vec!["name".to_owned()],
        unique: false,
        predicate: None,
    });
    let mut changed = before.clone();
    changed.indexes.last_mut().unwrap().unique = true;
    let plan = diff(&before, &changed);
    assert!(
        plan.changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveIndex { .. }))
    );
    assert!(
        plan.changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::AddIndex { .. }))
    );

    let removed = diff(&before, &after);
    assert!(
        removed
            .changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveIndex { .. }))
    );

    let mut with_seed = after.clone();
    with_seed.seeds.push(SeedSchema {
        id: "app::Project::demo".to_owned(),
        table: "projects".to_owned(),
        values: vec![SeedValueSchema {
            column: "id".to_owned(),
            value: "1".to_owned(),
            data_type: DatabaseType::Uuid,
        }],
    });
    let mut replaced = with_seed.clone();
    replaced.seeds[0].values[0].value = "2".to_owned();
    let seed_plan = diff(&with_seed, &replaced);
    assert!(
        seed_plan
            .changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveSeed { .. }))
    );
    let dropped = diff(&with_seed, &after);
    assert!(
        dropped
            .changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveSeed { .. }))
    );
}

#[test]
fn unique_column_addition_requires_manual_review() {
    let before = fixture_schema();
    let mut after = before.clone();
    after.tables[0].columns.push(ColumnSchema {
        id: "app::Project.slug".to_owned(),
        name: "slug".to_owned(),
        data_type: DatabaseType::Text,
        nullable: true,
        primary_key: false,
        unique: true,
        default: None,
        generated: None,
    });
    let plan = diff(&before, &after);
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.is_blocked());
    assert_eq!(plan.changes[0].risk.execution, ExecutionRisk::ManualReview);
}

#[test]
fn unique_constraint_and_foreign_key_replacements_are_diffed() {
    let after = fixture_schema();
    let mut before = after.clone();
    if let Some(constraint) = before.unique_constraints.first_mut() {
        constraint.columns.reverse();
    } else {
        before
            .unique_constraints
            .push(appstruct_migrate::UniqueConstraintSchema {
                id: "projects.tenant_key".to_owned(),
                table: "projects".to_owned(),
                columns: vec!["id".to_owned()],
            });
    }
    let plan = diff(&before, &after);
    assert!(plan.changes.iter().any(|change| matches!(
        change.change,
        SchemaChange::RemoveUniqueConstraint { .. } | SchemaChange::AddUniqueConstraint { .. }
    )));

    let mut without_fk = after.clone();
    let removed = without_fk.foreign_keys.pop().unwrap();
    let dropped = diff(&after, &without_fk);
    assert!(
        dropped
            .changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveForeignKey { .. }))
    );
    let mut replaced = after.clone();
    replaced.foreign_keys.last_mut().unwrap().unique = !removed.unique;
    let replaced_plan = diff(&after, &replaced);
    assert!(
        replaced_plan
            .changes
            .iter()
            .any(|change| matches!(change.change, SchemaChange::RemoveForeignKey { .. }))
    );
}

#[test]
fn generated_value_changes_require_input() {
    let before = fixture_schema();
    let mut after = before.clone();
    let created = after.tables[0]
        .columns
        .iter_mut()
        .find(|column| column.generated.is_some() || column.name.contains("created"))
        .unwrap();
    created.generated = match created.generated {
        Some(_) => None,
        None => Some(appstruct_ir::GeneratedValueIr::Now),
    };
    let plan = diff(&before, &after);
    assert!(plan.is_blocked());
    assert!(matches!(
        plan.changes[0].change,
        SchemaChange::AlterColumn { .. }
    ));
}
