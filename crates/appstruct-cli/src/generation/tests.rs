use super::ownership;
use super::transaction::{GenerationFault, GenerationTransaction};
use appstruct_codegen::{Artifact, ArtifactKind};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn project_lock_excludes_another_generation_process() {
    let temporary = tempfile::tempdir().unwrap();
    let first = GenerationTransaction::acquire(temporary.path()).unwrap();
    let second = GenerationTransaction::acquire(temporary.path());
    assert!(second.is_err());
    drop(first);
    GenerationTransaction::acquire(temporary.path()).unwrap();
}

#[test]
fn prepared_recovery_restores_the_previous_tree() {
    let temporary = tempfile::tempdir().unwrap();
    install(temporary.path(), "old");
    write_tree(
        &temporary.path().join(".generated.appstruct-staging"),
        "new",
    );
    write_journal(temporary.path(), "prepared");
    fs::rename(
        temporary.path().join("generated"),
        temporary.path().join(".generated.appstruct-backup"),
    )
    .unwrap();

    GenerationTransaction::acquire(temporary.path()).unwrap();
    assert_eq!(tree_value(&temporary.path().join("generated")), "old");
    assert_clean(temporary.path());
}

#[test]
fn backed_up_recovery_finishes_an_installed_tree() {
    let temporary = tempfile::tempdir().unwrap();
    install(temporary.path(), "old");
    write_tree(
        &temporary.path().join(".generated.appstruct-staging"),
        "new",
    );
    write_journal(temporary.path(), "backed_up");
    fs::rename(
        temporary.path().join("generated"),
        temporary.path().join(".generated.appstruct-backup"),
    )
    .unwrap();
    fs::rename(
        temporary.path().join(".generated.appstruct-staging"),
        temporary.path().join("generated"),
    )
    .unwrap();

    GenerationTransaction::acquire(temporary.path()).unwrap();
    assert_eq!(tree_value(&temporary.path().join("generated")), "new");
    assert_clean(temporary.path());
}

#[test]
fn journal_free_legacy_backup_is_recovered() {
    let temporary = tempfile::tempdir().unwrap();
    write_tree(&temporary.path().join(".generated.appstruct-backup"), "old");
    write_tree(
        &temporary.path().join(".generated.appstruct-staging"),
        "new",
    );

    GenerationTransaction::acquire(temporary.path()).unwrap();
    assert_eq!(tree_value(&temporary.path().join("generated")), "old");
    assert_clean(temporary.path());
}

#[test]
fn injected_swap_failures_recover_to_a_complete_generation() {
    for (fault, expected) in [
        (GenerationFault::AfterPrepared, "old"),
        (GenerationFault::AfterBackup, "old"),
        (GenerationFault::AfterInstall, "new"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        install(temporary.path(), "old");
        let transaction = GenerationTransaction::acquire(temporary.path()).unwrap();
        assert!(
            transaction
                .replace_with_fault(&files("new"), fault)
                .is_err()
        );
        assert_eq!(tree_value(&temporary.path().join("generated")), expected);
        assert_clean(temporary.path());
    }
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

fn write_journal(project: &Path, phase: &str) {
    let state = project.join(".appstruct");
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("generation.journal"),
        format!("{{\"version\":1,\"phase\":\"{phase}\"}}\n"),
    )
    .unwrap();
}

fn tree_value(root: &Path) -> String {
    fs::read_to_string(root.join("value.txt")).unwrap()
}

fn assert_clean(project: &Path) {
    assert!(!project.join(".generated.appstruct-staging").exists());
    assert!(!project.join(".generated.appstruct-backup").exists());
    assert!(!project.join(".appstruct/generation.journal").exists());
}
