use super::{atomic_replace, ensure_git_clean};
use crate::generation::ownership;
use appstruct_codegen::{Artifact, ArtifactKind};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn atomic_replace_swaps_lock_and_owned_generated_tree() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.lock"), "old lock\n").unwrap();
    fs::create_dir(project.path().join("app")).unwrap();
    fs::write(project.path().join("app/handler.rs"), "user code\n").unwrap();
    write_owned_tree(&project.path().join("generated"), "old");

    let staging = tempfile::tempdir_in(project.path()).unwrap();
    let staged_generated = staging.path().join("generated");
    write_owned_tree(&staged_generated, "new");

    atomic_replace(project.path(), &staged_generated, b"new lock\n").unwrap();

    assert_eq!(
        fs::read_to_string(project.path().join("generated/value.txt")).unwrap(),
        "new"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("appstruct.lock")).unwrap(),
        "new lock\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("app/handler.rs")).unwrap(),
        "user code\n"
    );
    assert!(!project.path().join(".generated.appstruct-old").exists());
}

#[test]
fn atomic_replace_rejects_tampered_staged_tree_without_mutating_project() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.lock"), "old lock\n").unwrap();
    write_owned_tree(&project.path().join("generated"), "old");

    let staging = tempfile::tempdir_in(project.path()).unwrap();
    let staged_generated = staging.path().join("generated");
    write_owned_tree(&staged_generated, "new");
    fs::write(staged_generated.join("value.txt"), "tampered").unwrap();

    assert!(atomic_replace(project.path(), &staged_generated, b"new lock\n").is_err());
    assert_eq!(
        fs::read_to_string(project.path().join("generated/value.txt")).unwrap(),
        "old"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("appstruct.lock")).unwrap(),
        "old lock\n"
    );
}

#[test]
fn ensure_git_clean_rejects_dirty_worktree_and_allows_clean_repo() {
    let project = tempfile::tempdir().unwrap();
    let init = Command::new("git")
        .arg("-C")
        .arg(project.path())
        .arg("init")
        .output();
    let Ok(init) = init else {
        return;
    };
    assert!(init.status.success());

    ensure_git_clean(project.path()).unwrap();

    fs::write(project.path().join("dirty.txt"), "uncommitted\n").unwrap();
    let err = ensure_git_clean(project.path()).unwrap_err();
    assert!(err.to_string().contains("dirty"));
}

fn write_owned_tree(root: &Path, value: &str) {
    let files: BTreeMap<PathBuf, Vec<u8>> = ownership::expected_files(&[Artifact {
        relative_path: PathBuf::from("value.txt"),
        content: value.as_bytes().to_vec(),
        executable: false,
        kind: ArtifactKind::CanonicalIr,
    }])
    .unwrap();
    for (relative, content) in files {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    ownership::validate_owned_tree(root).unwrap();
}
