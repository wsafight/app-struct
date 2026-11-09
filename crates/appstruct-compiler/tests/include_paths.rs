use appstruct_compiler::project_include_paths;
use std::{fs, path::PathBuf};

#[test]
fn include_paths_are_sorted_deduplicated_and_keep_missing_files() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "includes: [z.yaml, ./nested/../a.yaml, a.yaml]\n",
    )
    .unwrap();
    assert_eq!(
        project_include_paths(project.path()).unwrap(),
        vec![PathBuf::from("a.yaml"), PathBuf::from("z.yaml")]
    );
}

#[test]
fn include_paths_reject_absolute_and_parent_escapes() {
    let project = tempfile::tempdir().unwrap();
    for path in [
        "/outside.yaml",
        "../outside.yaml",
        "nested/../../outside.yaml",
    ] {
        fs::write(
            project.path().join("appstruct.yaml"),
            format!("includes: [{path}]\n"),
        )
        .unwrap();
        let diagnostics = project_include_paths(project.path()).unwrap_err();
        assert_eq!(diagnostics[0].code, "AS1010", "{path}");
    }
}

#[test]
#[cfg(unix)]
fn include_paths_track_directory_symlinks_and_resolve_parent_components() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("domains/nested")).unwrap();
    symlink("domains/nested", project.path().join("alias")).unwrap();
    fs::write(project.path().join("domains/main.yaml"), "domain: main\n").unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "includes: [alias/../main.yaml]\n",
    )
    .unwrap();
    assert_eq!(
        project_include_paths(project.path()).unwrap(),
        vec![PathBuf::from("alias"), PathBuf::from("domains/main.yaml")]
    );
}

#[test]
#[cfg(unix)]
fn include_paths_reject_missing_files_beneath_outside_symlinks() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), project.path().join("alias")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "includes: [alias/missing.yaml]\n",
    )
    .unwrap();
    let diagnostics = project_include_paths(project.path()).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS1010");
}
