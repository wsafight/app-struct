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
