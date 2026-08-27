use super::transaction::UpdateTransaction;
use super::workspace::CandidateWorkspace;
use crate::generation::{ownership, transaction::GenerationTransaction};
use crate::transaction::TransactionFault;
use appstruct_codegen::{Artifact, ArtifactKind};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn update_commits_lock_and_owned_generation_together() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.lock"), "old lock\n").unwrap();
    fs::create_dir(project.path().join("app")).unwrap();
    fs::write(project.path().join("app/handler.rs"), "user code\n").unwrap();
    install(project.path(), "old");
    let candidate = tempfile::tempdir().unwrap();
    write_tree(candidate.path(), "new");
    fs::create_dir_all(candidate.path().join("backend")).unwrap();
    fs::write(
        candidate.path().join("backend/Cargo.lock"),
        "generated lock\n",
    )
    .unwrap();

    let transaction = UpdateTransaction::acquire(project.path()).unwrap();
    transaction
        .commit(candidate.path(), b"canonical lock\n")
        .unwrap();

    assert_eq!(read_value(&project.path().join("generated")), "new");
    assert_eq!(
        fs::read_to_string(project.path().join("appstruct.lock")).unwrap(),
        "canonical lock\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("generated/backend/Cargo.lock")).unwrap(),
        "generated lock\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("app/handler.rs")).unwrap(),
        "user code\n"
    );
    assert_clean(project.path());
}

#[test]
fn invalid_candidate_leaves_current_project_unchanged() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.lock"), "old lock\n").unwrap();
    install(project.path(), "old");
    let candidate = tempfile::tempdir().unwrap();
    write_tree(candidate.path(), "new");
    fs::write(candidate.path().join("value.txt"), "tampered").unwrap();

    let transaction = UpdateTransaction::acquire(project.path()).unwrap();
    assert!(
        transaction
            .commit(candidate.path(), b"canonical lock\n")
            .is_err()
    );

    assert_eq!(read_value(&project.path().join("generated")), "old");
    assert_eq!(
        fs::read_to_string(project.path().join("appstruct.lock")).unwrap(),
        "old lock\n"
    );
    assert_clean(project.path());
}

#[test]
fn injected_update_failures_recover_lock_and_generation_together() {
    for (fault, expected) in [
        (TransactionFault::AfterPrepared, "old"),
        (TransactionFault::AfterBackup, "old"),
        (TransactionFault::AfterInstall, "new"),
    ] {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("appstruct.lock"), "old lock\n").unwrap();
        install(project.path(), "old");
        let candidate = tempfile::tempdir().unwrap();
        write_tree(candidate.path(), "new");

        let transaction = UpdateTransaction::acquire(project.path()).unwrap();
        assert!(
            transaction
                .commit_with_fault(candidate.path(), b"new lock\n", fault)
                .is_err()
        );

        assert_eq!(read_value(&project.path().join("generated")), expected);
        let expected_lock = if expected == "new" {
            "new lock\n"
        } else {
            "old lock\n"
        };
        assert_eq!(
            fs::read_to_string(project.path().join("appstruct.lock")).unwrap(),
            expected_lock
        );
        assert_clean(project.path());
    }
}

#[test]
fn backed_up_update_recovers_both_previous_inputs() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.lock"), "old lock\n").unwrap();
    install(project.path(), "old");
    fs::rename(
        project.path().join("appstruct.lock"),
        project
            .path()
            .join(".appstruct.lock.appstruct-update-backup"),
    )
    .unwrap();
    fs::rename(
        project.path().join("generated"),
        project.path().join(".generated.appstruct-update-backup"),
    )
    .unwrap();
    fs::write(
        project
            .path()
            .join(".appstruct.lock.appstruct-update-staging"),
        "new lock\n",
    )
    .unwrap();
    write_tree(
        &project.path().join(".generated.appstruct-update-staging"),
        "new",
    );
    fs::write(
        project.path().join(".appstruct/update.journal"),
        concat!(
            "{\"version\":1,\"phase\":\"backed_up\",",
            "\"had_lock\":true,\"had_generated\":true}\n"
        ),
    )
    .unwrap();

    UpdateTransaction::acquire(project.path()).unwrap();

    assert_eq!(read_value(&project.path().join("generated")), "old");
    assert_eq!(
        fs::read_to_string(project.path().join("appstruct.lock")).unwrap(),
        "old lock\n"
    );
    assert_clean(project.path());
}

#[test]
fn candidate_snapshot_detects_concurrent_source_changes() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.yaml"), "version: 1\n").unwrap();
    fs::create_dir(project.path().join("app")).unwrap();
    fs::write(project.path().join("app/handler.rs"), "before\n").unwrap();
    fs::create_dir_all(project.path().join("spec/target")).unwrap();
    fs::write(
        project.path().join("spec/target/domain.yaml"),
        "domain: Demo\n",
    )
    .unwrap();
    let candidate = CandidateWorkspace::prepare(project.path()).unwrap();
    assert_eq!(
        fs::read_to_string(candidate.path().join("app/handler.rs")).unwrap(),
        "before\n"
    );
    assert!(candidate.path().join("spec/target/domain.yaml").is_file());
    candidate.ensure_source_unchanged(project.path()).unwrap();

    fs::write(project.path().join("app/handler.rs"), "after\n").unwrap();
    assert!(candidate.ensure_source_unchanged(project.path()).is_err());
}

fn install(project: &Path, value: &str) {
    let transaction = GenerationTransaction::acquire(project).unwrap();
    transaction.replace(&files(value)).unwrap();
}

fn write_tree(root: &Path, value: &str) {
    for (relative, content) in files(value) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    ownership::validate_owned_tree(root).unwrap();
}

fn files(value: &str) -> BTreeMap<PathBuf, Vec<u8>> {
    ownership::expected_files(&[Artifact {
        relative_path: PathBuf::from("value.txt"),
        content: value.as_bytes().to_vec(),
        executable: false,
        kind: ArtifactKind::CanonicalIr,
    }])
    .unwrap()
}

fn read_value(root: &Path) -> String {
    fs::read_to_string(root.join("value.txt")).unwrap()
}

fn assert_clean(project: &Path) {
    for relative in [
        ".appstruct/update.journal",
        ".generated.appstruct-update-staging",
        ".generated.appstruct-update-backup",
        ".appstruct.lock.appstruct-update-staging",
        ".appstruct.lock.appstruct-update-backup",
    ] {
        assert!(!project.join(relative).exists(), "left {relative}");
    }
}
