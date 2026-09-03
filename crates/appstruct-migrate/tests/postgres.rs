use appstruct_ir::{DatabaseProvider, GeneratedValueIr, OnDeleteIr};
use appstruct_migrate::{
    ColumnSchema, DatabaseSchema, DatabaseType, DriftStatus, ForeignKeySchema, IndexSchema,
    TableSchema, UniqueConstraintSchema, apply_project, connect_database, initial_migration,
    inspect_database_schema, stamp_schema_checksum, status_project, to_json,
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
