use super::*;

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
