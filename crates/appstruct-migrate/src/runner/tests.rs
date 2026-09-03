use super::project::{directives, empty_schema};
use super::*;
use std::fs;

#[test]
fn stamped_migration_binds_to_snapshot() {
    let snapshot = "{\"schema_version\":1}\n";
    let sql = stamp_schema_checksum("-- generated\nCREATE TABLE demo ();\n", snapshot);
    assert!(
        sql.lines()
            .nth(1)
            .unwrap()
            .starts_with(SCHEMA_CHECKSUM_PREFIX)
    );
    assert!(directives("0001_demo.sql", &sql).unwrap().1.is_some());
}

#[test]
fn rejects_modified_applied_migration() {
    let files = vec![MigrationFile {
        id: "0001_demo.sql".to_owned(),
        checksum: "new".to_owned(),
        sql: String::new(),
        transactional: true,
        schema_checksum: None,
    }];
    let applied = vec![history::HistoryEntry {
        id: "0001_demo.sql".to_owned(),
        checksum: "old".to_owned(),
        state: "applied".to_owned(),
    }];
    assert!(matches!(
        reconcile(&files, &applied),
        Err(MigrationError::Integrity(message)) if message.contains("modified")
    ));
}

#[test]
fn rejects_dirty_nontransactional_history() {
    let files = vec![MigrationFile {
        id: "0001_demo.sql".to_owned(),
        checksum: "same".to_owned(),
        sql: String::new(),
        transactional: false,
        schema_checksum: None,
    }];
    let applied = vec![history::HistoryEntry {
        id: "0001_demo.sql".to_owned(),
        checksum: "same".to_owned(),
        state: "failed".to_owned(),
    }];
    assert!(matches!(
        reconcile(&files, &applied),
        Err(MigrationError::Integrity(message)) if message.contains("manual recovery")
    ));
}

#[test]
fn parses_transaction_boundary_directive() {
    let parsed = directives(
        "0001_index.sql",
        "-- appstruct:transaction=off\nCREATE INDEX CONCURRENTLY demo;\n",
    )
    .unwrap();
    assert!(!parsed.0);
    assert!(matches!(
        directives("0001_index.sql", "-- appstruct:transaction=maybe\n"),
        Err(MigrationError::Project(message)) if message.contains("invalid transaction")
    ));
}

#[test]
fn loads_snapshot_bound_migrations_in_filename_order() {
    let temporary = tempfile::tempdir().unwrap();
    let migrations = temporary.path().join("migrations");
    let state = temporary.path().join(".appstruct");
    fs::create_dir_all(&migrations).unwrap();
    fs::create_dir_all(&state).unwrap();
    let snapshot = crate::to_json(&empty_schema()).unwrap();
    fs::write(state.join("schema.snapshot.json"), &snapshot).unwrap();
    fs::write(
        migrations.join("0001_first.sql"),
        stamp_schema_checksum("SELECT 1;\n", &snapshot),
    )
    .unwrap();
    fs::write(
        migrations.join("0002_second.sql"),
        stamp_schema_checksum("SELECT 2;\n", &snapshot),
    )
    .unwrap();

    let project = ProjectMigrations::load(temporary.path()).unwrap();
    assert_eq!(project.files[0].id, "0001_first.sql");
    assert_eq!(project.files[1].id, "0002_second.sql");
}

#[test]
fn stamp_without_newline_appends_checksum_on_the_next_line() {
    let stamped = stamp_schema_checksum("SELECT 1;", "{\"schema_version\":1}");
    assert!(stamped.starts_with("SELECT 1;\n-- appstruct:schema-sha256="));
}

#[test]
fn migration_error_display_preserves_the_inner_message() {
    for error in [
        MigrationError::Project("project".to_owned()),
        MigrationError::Database("database".to_owned()),
        MigrationError::Integrity("integrity".to_owned()),
    ] {
        assert_eq!(
            error.to_string(),
            match &error {
                MigrationError::Project(message)
                | MigrationError::Database(message)
                | MigrationError::Integrity(message) => message.as_str(),
            }
        );
    }
}

#[test]
fn connect_database_rejects_invalid_urls_and_unreachable_hosts() {
    assert!(matches!(
        connect_database("not-a-postgres-url"),
        Err(MigrationError::Database(message)) if message.contains("invalid PostgreSQL")
    ));
    assert!(matches!(
        connect_database("postgresql://appstruct:secret@127.0.0.1:1/appstruct?sslmode=disable"),
        Err(MigrationError::Database(_))
    ));
    assert!(matches!(
        connect_database("postgresql://appstruct:secret@127.0.0.1:1/appstruct"),
        Err(MigrationError::Database(_))
    ));
}

#[test]
fn status_and_apply_fail_before_connecting_when_the_project_is_invalid() {
    let missing = tempfile::tempdir().unwrap();
    fs::write(missing.path().join("migrations"), "not-a-directory").unwrap();
    assert!(matches!(
        status_project(
            missing.path(),
            "postgresql://127.0.0.1:1/appstruct?sslmode=disable"
        ),
        Err(MigrationError::Project(_))
    ));
    assert!(matches!(
        apply_project(
            missing.path(),
            "postgresql://127.0.0.1:1/appstruct?sslmode=disable"
        ),
        Err(MigrationError::Project(_))
    ));
}

#[test]
fn status_and_apply_report_database_errors_for_empty_projects() {
    let project = tempfile::tempdir().unwrap();
    let url = "postgresql://appstruct:secret@127.0.0.1:1/appstruct?sslmode=disable";
    assert!(matches!(
        status_project(project.path(), url),
        Err(MigrationError::Database(_))
    ));
    assert!(matches!(
        apply_project(project.path(), url),
        Err(MigrationError::Database(_))
    ));
}

#[test]
fn load_rejects_snapshot_without_migrations_and_checksum_mismatch() {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir_all(temporary.path().join(".appstruct")).unwrap();
    fs::write(
        temporary.path().join(".appstruct/schema.snapshot.json"),
        "{not json",
    )
    .unwrap();
    assert!(matches!(
        ProjectMigrations::load(temporary.path()),
        Err(MigrationError::Project(message)) if message.contains("invalid schema snapshot")
    ));

    let snapshot = crate::to_json(&empty_schema()).unwrap();
    fs::write(
        temporary.path().join(".appstruct/schema.snapshot.json"),
        &snapshot,
    )
    .unwrap();
    assert!(ProjectMigrations::load(temporary.path()).is_ok());

    let mut schema = empty_schema();
    schema.tables.push(crate::TableSchema {
        id: "notes".to_owned(),
        name: "notes".to_owned(),
        columns: Vec::new(),
    });
    fs::write(
        temporary.path().join(".appstruct/schema.snapshot.json"),
        crate::to_json(&schema).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        ProjectMigrations::load(temporary.path()),
        Err(MigrationError::Project(message)) if message.contains("no corresponding migration")
    ));

    fs::create_dir_all(temporary.path().join("migrations")).unwrap();
    fs::write(
        temporary.path().join("migrations/0001_demo.sql"),
        "SELECT 1;\n",
    )
    .unwrap();
    fs::write(
        temporary.path().join(".appstruct/schema.snapshot.json"),
        &snapshot,
    )
    .unwrap();
    assert!(matches!(
        ProjectMigrations::load(temporary.path()),
        Err(MigrationError::Integrity(message)) if message.contains("does not match")
    ));
}

#[test]
fn load_rejects_invalid_migration_filenames_and_duplicate_checksum_directives() {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir_all(temporary.path().join("migrations")).unwrap();
    fs::write(
        temporary.path().join("migrations/bad name.sql"),
        "SELECT 1;\n",
    )
    .unwrap();
    assert!(matches!(
        ProjectMigrations::load(temporary.path()),
        Err(MigrationError::Project(message)) if message.contains("invalid migration filename")
    ));

    fs::remove_file(temporary.path().join("migrations/bad name.sql")).unwrap();
    let checksum = "a".repeat(64);
    fs::write(
        temporary.path().join("migrations/0001_demo.sql"),
        format!("-- appstruct:schema-sha256={checksum}\n-- appstruct:schema-sha256={checksum}\n"),
    )
    .unwrap();
    assert!(matches!(
        ProjectMigrations::load(temporary.path()),
        Err(MigrationError::Project(message)) if message.contains("invalid schema checksum")
    ));
}

#[test]
fn reconcile_rejects_missing_out_of_order_and_extra_history() {
    let files = vec![MigrationFile {
        id: "0001_demo.sql".to_owned(),
        checksum: "same".to_owned(),
        sql: String::new(),
        transactional: true,
        schema_checksum: None,
    }];
    assert!(matches!(
        reconcile(
            &files,
            &[history::HistoryEntry {
                id: "0002_other.sql".to_owned(),
                checksum: "same".to_owned(),
                state: "applied".to_owned(),
            }]
        ),
        Err(MigrationError::Integrity(message)) if message.contains("out of order")
    ));
    assert!(matches!(
        reconcile(
            &[],
            &[history::HistoryEntry {
                id: "0001_demo.sql".to_owned(),
                checksum: "same".to_owned(),
                state: "applied".to_owned(),
            }]
        ),
        Err(MigrationError::Integrity(message)) if message.contains("missing from disk")
    ));
}
