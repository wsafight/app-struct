use super::*;

#[test]
fn output_paths_are_project_relative_and_never_overwritten() {
    let project = tempfile::tempdir().unwrap();
    assert!(
        resolve_output(
            project.path(),
            Path::new("spec/imported.yaml"),
            PullMode::Create
        )
        .is_ok()
    );
    assert!(
        resolve_output(
            project.path(),
            Path::new("../outside.yaml"),
            PullMode::Create
        )
        .is_err()
    );
    assert!(
        resolve_output(
            project.path(),
            Path::new("spec/imported.json"),
            PullMode::Create
        )
        .is_err()
    );
    fs::create_dir(project.path().join("spec")).unwrap();
    fs::write(project.path().join("spec/imported.yaml"), "keep\n").unwrap();
    assert!(
        resolve_output(
            project.path(),
            Path::new("spec/imported.yaml"),
            PullMode::Create
        )
        .is_err()
    );
    assert!(
        resolve_output(
            project.path(),
            Path::new("spec/imported.yaml"),
            PullMode::Check
        )
        .is_ok()
    );
}

#[test]
fn schema_names_reject_ambiguous_values() {
    assert!(validate_schema_name("public").is_ok());
    assert!(validate_schema_name("").is_err());
    assert!(validate_schema_name(" public").is_err());
    assert!(validate_schema_name(&"x".repeat(64)).is_err());
    assert!(validate_schema_name("pub\nlic").is_err());
}

#[test]
fn resolve_output_rejects_symlinked_and_file_parents() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("file.yaml"), "x\n").unwrap();
    assert!(
        resolve_output(
            project.path(),
            Path::new("/tmp/imported.yaml"),
            PullMode::Create
        )
        .is_err()
    );
    fs::write(project.path().join("not-a-dir"), "x\n").unwrap();
    assert!(
        resolve_output(
            project.path(),
            Path::new("not-a-dir/imported.yaml"),
            PullMode::Create
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("not-a-dir", project.path().join("link-dir")).unwrap();
        assert!(
            resolve_output(
                project.path(),
                Path::new("link-dir/imported.yaml"),
                PullMode::Create
            )
            .is_err()
        );
    }
}

#[test]
fn comparison_modes_require_existing_files_and_render_unified_diffs() {
    let project = tempfile::tempdir().unwrap();
    assert!(
        resolve_output(
            project.path(),
            Path::new("spec/imported.yaml"),
            PullMode::Diff
        )
        .is_err()
    );
    let diff = render_diff(Path::new("spec/imported.yaml"), "old\n", "new\n");
    assert!(diff.contains("--- spec/imported.yaml"));
    assert!(diff.contains("+++ live PostgreSQL schema"));
    assert!(diff.contains("-old"));
    assert!(diff.contains("+new"));
}

#[test]
fn pull_rejects_invalid_schema_names_and_output_paths() {
    let project = tempfile::tempdir().unwrap();
    for (schema, output) in [
        ("", "spec/imported.yaml"),
        ("public", "../outside.yaml"),
        ("public", "spec/imported.yaml"),
    ] {
        assert_ne!(
            run(
                project.path(),
                &DbCommand::Pull {
                    schema: schema.to_owned(),
                    output: PathBuf::from(output),
                    check: false,
                    diff: false,
                },
            ),
            ExitCode::SUCCESS
        );
    }
}
