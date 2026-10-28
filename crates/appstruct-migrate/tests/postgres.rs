use appstruct_ir::{
    DatabaseProvider, GeneratedValueIr, IndexMethod, IndexOperatorClass, OnDeleteIr,
};
use appstruct_migrate::{
    ColumnSchema, DatabaseSchema, DatabaseType, DriftStatus, ForeignKeySchema, IndexSchema,
    TableSchema, UniqueConstraintSchema, apply_project, baseline_project, connect_database, diff,
    initial_migration, inspect_baseline, inspect_database_schema, migration_sql,
    stamp_schema_checksum, status_project, to_json,
};
use std::fs;
use std::sync::Mutex;

static DATABASE: Mutex<()> = Mutex::new(());

fn database_url() -> Option<String> {
    std::env::var("APPSTRUCT_E2E_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn lock_database() -> std::sync::MutexGuard<'static, ()> {
    DATABASE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn reset_public(url: &str) {
    let mut client = connect_database(url).unwrap();
    client
        .batch_execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")
        .unwrap();
}

fn notes_schema() -> DatabaseSchema {
    DatabaseSchema {
        schema_version: appstruct_migrate::SCHEMA_VERSION,
        provider: DatabaseProvider::Postgres,
        tables: vec![TableSchema {
            id: "notes".to_owned(),
            name: "notes".to_owned(),
            columns: vec![
                ColumnSchema {
                    id: "notes.id".to_owned(),
                    name: "id".to_owned(),
                    data_type: DatabaseType::Uuid,
                    nullable: false,
                    primary_key: true,
                    unique: false,
                    default: None,
                    generated: Some(GeneratedValueIr::UuidV7),
                },
                ColumnSchema {
                    id: "notes.title".to_owned(),
                    name: "title".to_owned(),
                    data_type: DatabaseType::Text,
                    nullable: false,
                    primary_key: false,
                    unique: true,
                    default: None,
                    generated: None,
                },
                ColumnSchema {
                    id: "notes.status".to_owned(),
                    name: "status".to_owned(),
                    data_type: DatabaseType::Enum {
                        values: vec!["draft".to_owned(), "published".to_owned()],
                    },
                    nullable: false,
                    primary_key: false,
                    unique: false,
                    default: Some("draft".to_owned()),
                    generated: None,
                },
            ],
        }],
        unique_constraints: Vec::new(),
        indexes: vec![IndexSchema {
            id: "notes::published".to_owned(),
            table: "notes".to_owned(),
            columns: vec!["id".to_owned()],
            unique: false,
            method: appstruct_ir::IndexMethod::Btree,
            operator_class: None,
            predicate: Some("status = 'published'".to_owned()),
        }],
        seeds: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

fn write_project(schema: &DatabaseSchema) -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("migrations")).unwrap();
    fs::create_dir_all(project.path().join(".appstruct")).unwrap();
    let snapshot = to_json(schema).unwrap();
    fs::write(
        project.path().join(".appstruct/schema.snapshot.json"),
        &snapshot,
    )
    .unwrap();
    let sql = stamp_schema_checksum(&initial_migration(schema), &snapshot);
    fs::write(project.path().join("migrations/0001_notes.sql"), sql).unwrap();
    project
}

#[test]
fn baseline_preserves_populated_tables_and_allows_the_next_migration() {
    let Some(url) = database_url() else {
        return;
    };
    let _lock = lock_database();
    reset_public(&url);
    let schema = notes_schema();
    let mut client = connect_database(&url).unwrap();
    client.batch_execute(&initial_migration(&schema)).unwrap();
    client.batch_execute("INSERT INTO notes (id, title) VALUES ('00000000-0000-4000-8000-000000000001', 'existing row')").unwrap();
    let empty_project = tempfile::tempdir().unwrap();
    let preview = inspect_baseline(empty_project.path(), &url, &schema).unwrap();
    assert_eq!(preview.drift, DriftStatus::Clean);
    assert!(!preview.registered);
    assert!(preview.migration.is_none());
    assert_eq!(fs::read_dir(empty_project.path()).unwrap().count(), 0);
    let history_exists: bool = client
        .query_one(
            "SELECT to_regclass('_appstruct_migrations') IS NOT NULL",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(!history_exists);

    let project = write_project(&schema);
    for _ in 0..2 {
        let status = baseline_project(project.path(), &url, &schema).unwrap();
        assert!(status.registered);
        assert_eq!(status.drift, DriftStatus::Clean);
    }
    let status = status_project(project.path(), &url).unwrap();
    assert_eq!((status.applied, status.pending), (1, 0));
    assert_eq!(status.drift, DriftStatus::Clean);
    assert_eq!(
        client
            .query_one("SELECT title FROM notes", &[])
            .unwrap()
            .get::<_, String>(0),
        "existing row"
    );
    let mut next = schema.clone();
    let mut column = next.tables[0].columns[1].clone();
    column.id = "notes.description".to_owned();
    column.name = "description".to_owned();
    column.nullable = true;
    column.unique = false;
    next.tables[0].columns.push(column);
    let snapshot = to_json(&next).unwrap();
    fs::write(
        project.path().join(".appstruct/schema.snapshot.json"),
        &snapshot,
    )
    .unwrap();
    fs::write(
        project.path().join("migrations/0002_description.sql"),
        stamp_schema_checksum(&migration_sql(&diff(&schema, &next)).unwrap(), &snapshot),
    )
    .unwrap();
    let report = apply_project(project.path(), &url).unwrap();
    assert_eq!(report.applied_now, 1);
    assert_eq!(report.drift, DriftStatus::Clean);
    assert_eq!(
        client
            .query_one("SELECT title, description FROM notes", &[])
            .unwrap()
            .get::<_, Option<String>>(1),
        None
    );
}

#[test]
fn baseline_refuses_drift_and_tampered_history_without_registration() {
    let Some(url) = database_url() else {
        return;
    };
    let _lock = lock_database();
    reset_public(&url);
    let schema = notes_schema();
    let project = write_project(&schema);
    let mut client = connect_database(&url).unwrap();
    client.batch_execute(&initial_migration(&schema)).unwrap();
    client
        .batch_execute("ALTER TABLE notes ADD COLUMN extra TEXT")
        .unwrap();
    assert!(matches!(
        inspect_baseline(project.path(), &url, &schema)
            .unwrap()
            .drift,
        DriftStatus::Detected(_)
    ));
    assert!(
        baseline_project(project.path(), &url, &schema)
            .unwrap_err()
            .to_string()
            .contains("unexpected column")
    );
    assert!(
        !client
            .query_one(
                "SELECT to_regclass('_appstruct_migrations') IS NOT NULL",
                &[]
            )
            .unwrap()
            .get::<_, bool>(0)
    );
    client
        .batch_execute("ALTER TABLE notes DROP COLUMN extra")
        .unwrap();
    baseline_project(project.path(), &url, &schema).unwrap();
    client
        .batch_execute("UPDATE _appstruct_migrations SET checksum = 'tampered'")
        .unwrap();
    assert!(
        baseline_project(project.path(), &url, &schema)
            .unwrap_err()
            .to_string()
            .contains("modified after execution")
    );
    client
        .batch_execute("UPDATE _appstruct_migrations SET state = 'failed'")
        .unwrap();
    assert!(
        baseline_project(project.path(), &url, &schema)
            .unwrap_err()
            .to_string()
            .contains("manual recovery")
    );
}

#[test]
fn baseline_never_skips_arbitrary_sql_or_a_migration_chain() {
    let schema = notes_schema();
    let project = write_project(&schema);
    let path = project.path().join("migrations/0001_notes.sql");
    let sql = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{sql}\nDELETE FROM notes;\n")).unwrap();
    let error = inspect_baseline(project.path(), "invalid url", &schema).unwrap_err();
    assert!(error.to_string().contains("cannot skip arbitrary SQL"));
    fs::write(&path, &sql).unwrap();
    fs::write(project.path().join("migrations/0002_other.sql"), &sql).unwrap();
    assert!(
        baseline_project(project.path(), "invalid url", &schema)
            .unwrap_err()
            .to_string()
            .contains("migration chains")
    );
}

#[test]
fn baseline_refuses_seed_data_instead_of_silently_marking_it_applied() {
    let mut schema = notes_schema();
    schema.seeds.push(appstruct_migrate::SeedSchema {
        id: "notes.initial".to_owned(),
        table: "notes".to_owned(),
        values: Vec::new(),
    });
    let project = tempfile::tempdir().unwrap();
    assert!(
        inspect_baseline(project.path(), "invalid url", &schema)
            .unwrap_err()
            .to_string()
            .contains("without seeds")
    );
    assert_eq!(fs::read_dir(project.path()).unwrap().count(), 0);
}

#[test]
fn inspect_reports_missing_schema_and_reads_live_catalog() {
    let Some(url) = database_url() else {
        return;
    };
    let _lock = lock_database();
    reset_public(&url);
    let missing = inspect_database_schema(&url, "does_not_exist").unwrap_err();
    assert!(missing.to_string().contains("does not exist"));

    let mut client = connect_database(&url).unwrap();
    client
        .batch_execute(
            r"
CREATE TABLE notes (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published'))
);
CREATE INDEX idx_published ON notes (id) WHERE (status = 'published');
CREATE TABLE tasks (
    id UUID PRIMARY KEY,
    note_id UUID NOT NULL REFERENCES notes(id) ON DELETE CASCADE
);
",
        )
        .unwrap();

    let inspected = inspect_database_schema(&url, "public").unwrap();
    assert_eq!(inspected.name, "public");
    let notes = inspected
        .tables
        .iter()
        .find(|table| table.name == "notes")
        .unwrap();
    assert!(notes.columns.iter().any(|column| column.name == "status"));
    assert_eq!(notes.primary_key, ["id"]);
    assert!(notes.indexes.iter().any(|index| index.predicate.is_some()));
    assert!(
        inspected
            .foreign_keys
            .iter()
            .any(|key| key.source_table == "tasks" && key.on_delete.contains("cascade"))
    );
}

#[test]
fn apply_and_status_detect_clean_state_and_schema_drift() {
    let Some(url) = database_url() else {
        return;
    };
    let _lock = lock_database();
    reset_public(&url);
    let schema = notes_schema();
    let project = write_project(&schema);

    let status = status_project(project.path(), &url).unwrap();
    assert_eq!(status.applied, 0);
    assert_eq!(status.pending, 1);
    assert_eq!(status.drift, DriftStatus::Deferred);

    let report = apply_project(project.path(), &url).unwrap();
    assert_eq!(report.applied_now, 1);
    assert_eq!(report.total_applied, 1);
    assert_eq!(report.drift, DriftStatus::Clean);

    let status = status_project(project.path(), &url).unwrap();
    assert_eq!(status.applied, 1);
    assert_eq!(status.pending, 0);
    assert_eq!(status.drift, DriftStatus::Clean);

    let mut client = connect_database(&url).unwrap();
    client
        .batch_execute("ALTER TABLE notes ADD COLUMN extra TEXT;")
        .unwrap();
    let status = status_project(project.path(), &url).unwrap();
    match status.drift {
        DriftStatus::Detected(issues) => {
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.contains("unexpected column `notes.extra`")),
                "{issues:?}"
            );
        }
        other => panic!("expected drift, got {other:?}"),
    }
}

#[test]
fn unique_constraints_and_restrict_foreign_keys_are_inspected() {
    let Some(url) = database_url() else {
        return;
    };
    let _lock = lock_database();
    reset_public(&url);
    let mut schema = notes_schema();
    schema.tables.push(TableSchema {
        id: "authors".to_owned(),
        name: "authors".to_owned(),
        columns: vec![ColumnSchema {
            id: "authors.id".to_owned(),
            name: "id".to_owned(),
            data_type: DatabaseType::Uuid,
            nullable: false,
            primary_key: true,
            unique: false,
            default: None,
            generated: Some(GeneratedValueIr::UuidV7),
        }],
    });
    schema.tables[0].columns.push(ColumnSchema {
        id: "notes.author_id".to_owned(),
        name: "author_id".to_owned(),
        data_type: DatabaseType::Uuid,
        nullable: false,
        primary_key: false,
        unique: false,
        default: None,
        generated: None,
    });
    schema.unique_constraints.push(UniqueConstraintSchema {
        id: "notes.title_status".to_owned(),
        table: "notes".to_owned(),
        columns: vec!["title".to_owned(), "status".to_owned()],
    });
    schema.foreign_keys.push(ForeignKeySchema {
        id: "notes.author".to_owned(),
        source_table: "notes".to_owned(),
        source_columns: vec!["author_id".to_owned()],
        target_table: "authors".to_owned(),
        target_columns: vec!["id".to_owned()],
        unique: false,
        on_delete: OnDeleteIr::Restrict,
    });
    let project = write_project(&schema);
    apply_project(project.path(), &url).unwrap();
    let inspected = inspect_database_schema(&url, "public").unwrap();
    assert!(inspected.tables.iter().any(|table| table.name == "authors"));
    assert!(
        inspected
            .foreign_keys
            .iter()
            .any(|key| key.source_columns == ["author_id"] && key.on_delete.contains("restrict"))
    );
    let status = status_project(project.path(), &url).unwrap();
    assert_eq!(status.drift, DriftStatus::Clean);
}

#[test]
fn gin_indexes_round_trip_and_detect_method_and_operator_class_drift() {
    let Some(url) = database_url() else {
        return;
    };
    let _lock = lock_database();
    reset_public(&url);
    let mut schema = notes_schema();
    schema.tables[0].columns.push(ColumnSchema {
        id: "notes.payload".to_owned(),
        name: "payload".to_owned(),
        data_type: DatabaseType::Json,
        nullable: true,
        primary_key: false,
        unique: false,
        default: None,
        generated: None,
    });
    for (name, column, class) in [
        ("title_search", "title", IndexOperatorClass::GinTrgmOps),
        ("payload_search", "payload", IndexOperatorClass::JsonbOps),
    ] {
        schema.indexes.push(IndexSchema {
            id: format!("notes::{name}"),
            table: "notes".to_owned(),
            columns: vec![column.to_owned()],
            unique: false,
            method: IndexMethod::Gin,
            operator_class: Some(class),
            predicate: None,
        });
    }
    let project = write_project(&schema);
    apply_project(project.path(), &url).unwrap();
    assert_eq!(
        status_project(project.path(), &url).unwrap().drift,
        DriftStatus::Clean
    );
    let inspected = inspect_database_schema(&url, "public").unwrap();
    let notes = inspected
        .tables
        .iter()
        .find(|table| table.name == "notes")
        .unwrap();
    assert!(
        notes.indexes.iter().any(|index| index.method == "gin"
            && index.operator_class.as_deref() == Some("gin_trgm_ops"))
    );
    assert!(
        notes
            .indexes
            .iter()
            .any(|index| index.operator_class.as_deref() == Some("jsonb_ops"))
    );
    let mut client = connect_database(&url).unwrap();
    client
        .batch_execute(
            "DROP INDEX idx_title_search; CREATE INDEX idx_title_search ON notes (title)",
        )
        .unwrap();
    assert_ne!(
        status_project(project.path(), &url).unwrap().drift,
        DriftStatus::Clean
    );
    client.batch_execute("DROP INDEX idx_title_search; CREATE INDEX idx_title_search ON notes USING gin (title gin_trgm_ops); DROP INDEX idx_payload_search; CREATE INDEX idx_payload_search ON notes USING gin (payload jsonb_path_ops)").unwrap();
    assert_ne!(
        status_project(project.path(), &url).unwrap().drift,
        DriftStatus::Clean
    );
}
