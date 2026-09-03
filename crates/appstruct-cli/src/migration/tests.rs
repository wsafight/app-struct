use super::transaction::next_migration_name;
use super::*;
use crate::transaction::TransactionFault;

#[test]
fn migration_sequence_uses_highest_existing_prefix() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(temporary.path().join("0001_first.sql"), "").unwrap();
    fs::write(temporary.path().join("0003_third.sql"), "").unwrap();
    fs::write(temporary.path().join("notes.txt"), "").unwrap();

    assert_eq!(
        next_migration_name(temporary.path()).unwrap(),
        "0004_appstruct.sql"
    );
}

#[test]
fn migration_commit_refuses_existing_staging_files() {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("migrations")).unwrap();
    fs::create_dir(temporary.path().join(".appstruct")).unwrap();
    let staging = temporary.path().join("migrations/0001_appstruct.sql.tmp");
    fs::write(&staging, "preserve me\n").unwrap();

    let error = write_plan(temporary.path(), "SELECT 1;\n", "{}\n").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(staging).unwrap(), "preserve me\n");
}

#[test]
fn injected_migration_failures_recover_snapshot_and_plan_together() {
    for (fault, installed) in [
        (TransactionFault::AfterPrepared, false),
        (TransactionFault::AfterBackup, false),
        (TransactionFault::AfterInstall, true),
    ] {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join(".appstruct")).unwrap();
        fs::write(
            project.path().join(".appstruct/schema.snapshot.json"),
            "old snapshot\n",
        )
        .unwrap();

        let transaction = MigrationTransaction::acquire(project.path()).unwrap();
        assert!(
            transaction
                .commit_with_fault("SELECT 1;\n", "new snapshot\n", fault)
                .is_err()
        );

        let expected = if installed {
            "new snapshot\n"
        } else {
            "old snapshot\n"
        };
        assert_eq!(
            fs::read_to_string(project.path().join(".appstruct/schema.snapshot.json")).unwrap(),
            expected
        );
        let migrations = fs::read_dir(project.path().join("migrations"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|value| value == "sql"))
            .count();
        assert_eq!(migrations, usize::from(installed));
        assert!(!project.path().join(".appstruct/migration.journal").exists());
        assert!(
            !project
                .path()
                .join(".appstruct/schema.snapshot.json.appstruct-backup")
                .exists()
        );
        assert!(
            !project
                .path()
                .join(".appstruct/schema.snapshot.json.tmp")
                .exists()
        );
    }
}

#[test]
fn plan_and_lint_cover_empty_and_fixture_projects() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    assert_eq!(run(&fixture, MigrateCommand::Plan), ExitCode::SUCCESS);
    assert_eq!(
        run(
            &fixture,
            MigrateCommand::Lint {
                deny_warnings: false
            }
        ),
        ExitCode::SUCCESS
    );
    crate::report::set_output_format(crate::report::OutputFormat::Json);
    assert_eq!(run(&fixture, MigrateCommand::Plan), ExitCode::SUCCESS);
    assert_eq!(
        run(
            &fixture,
            MigrateCommand::Lint {
                deny_warnings: true
            }
        ),
        ExitCode::SUCCESS
    );
    crate::report::set_output_format(crate::report::OutputFormat::Text);
}

#[test]
fn apply_and_status_require_database_url() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    assert_ne!(
        run_with_database(&fixture, MigrateCommand::Apply, Some("")),
        ExitCode::SUCCESS
    );
    assert_ne!(
        run_with_database(&fixture, MigrateCommand::Status, Some("")),
        ExitCode::SUCCESS
    );
}

#[test]
fn change_labels_cover_every_schema_change_kind() {
    use appstruct_migrate::{
        ColumnSchema, ForeignKeySchema, IndexSchema, SeedSchema, TableSchema,
        UniqueConstraintSchema,
    };
    let table = TableSchema {
        id: "notes".to_owned(),
        name: "notes".to_owned(),
        columns: Vec::new(),
    };
    let column = ColumnSchema {
        id: "notes.id".to_owned(),
        name: "id".to_owned(),
        data_type: appstruct_migrate::DatabaseType::Uuid,
        nullable: false,
        primary_key: true,
        unique: false,
        default: None,
        generated: None,
    };
    let constraint = UniqueConstraintSchema {
        id: "notes.email".to_owned(),
        table: "notes".to_owned(),
        columns: vec!["email".to_owned()],
    };
    let index = IndexSchema {
        id: "notes.email".to_owned(),
        table: "notes".to_owned(),
        columns: vec!["email".to_owned()],
        unique: false,
        predicate: None,
    };
    let seed = SeedSchema {
        id: "notes.demo".to_owned(),
        table: "notes".to_owned(),
        values: Vec::new(),
    };
    let foreign_key = ForeignKeySchema {
        id: "notes.author".to_owned(),
        source_table: "notes".to_owned(),
        source_columns: vec!["author_id".to_owned()],
        target_table: "users".to_owned(),
        target_columns: vec!["id".to_owned()],
        unique: false,
        on_delete: appstruct_ir::OnDeleteIr::Restrict,
    };
    for change in [
        SchemaChange::AddTable {
            table: table.clone(),
        },
        SchemaChange::RemoveTable {
            table: table.clone(),
        },
        SchemaChange::RenameTable {
            before: table.clone(),
            after: table,
        },
        SchemaChange::AddColumn {
            table: "notes".to_owned(),
            column: column.clone(),
        },
        SchemaChange::RemoveColumn {
            table: "notes".to_owned(),
            column: column.clone(),
        },
        SchemaChange::AlterColumn {
            table: "notes".to_owned(),
            before: column.clone(),
            after: column,
        },
        SchemaChange::AddUniqueConstraint {
            constraint: constraint.clone(),
        },
        SchemaChange::RemoveUniqueConstraint { constraint },
        SchemaChange::AddIndex {
            index: index.clone(),
        },
        SchemaChange::RemoveIndex { index },
        SchemaChange::AddSeed { seed: seed.clone() },
        SchemaChange::RemoveSeed { seed },
        SchemaChange::AddForeignKey {
            foreign_key: foreign_key.clone(),
        },
        SchemaChange::RemoveForeignKey { foreign_key },
    ] {
        assert!(!change_label(&change).is_empty());
    }
}

#[test]
fn invalid_projects_and_dev_without_accept_are_rejected() {
    assert_ne!(
        run(
            Path::new("/missing-appstruct-project"),
            MigrateCommand::Plan
        ),
        ExitCode::SUCCESS
    );
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let _ = run(&fixture, MigrateCommand::Dev { accept: false });
    crate::report::set_output_format(crate::report::OutputFormat::Json);
    render_dev_success(None, true, false);
    render_dev_success(Some(Path::new("migrations/0001.sql")), false, true);
    render_plan(&appstruct_migrate::MigrationPlan {
        changes: Vec::new(),
    });
    crate::report::set_output_format(crate::report::OutputFormat::Text);
    render_plan(&appstruct_migrate::MigrationPlan {
        changes: Vec::new(),
    });
    assert!(read_snapshot(Path::new("/missing")).unwrap().is_none());
}
