use super::*;

#[test]
fn source_fingerprints_classify_project_inputs() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(temporary.path().join("appstruct.yaml"), "version: 1\n").unwrap();
    let first = SourceFingerprints::read(temporary.path()).unwrap();
    fs::create_dir(temporary.path().join("generated")).unwrap();
    fs::write(temporary.path().join("generated/output"), "ignored").unwrap();
    assert_eq!(first, SourceFingerprints::read(temporary.path()).unwrap());

    fs::create_dir(temporary.path().join("spec")).unwrap();
    fs::write(temporary.path().join("spec/main.yaml"), "domain: main\n").unwrap();
    let specification = SourceFingerprints::read(temporary.path()).unwrap();
    assert!(first.changes(&specification).specification);
    assert!(!first.changes(&specification).backend);
    assert!(!first.changes(&specification).web);

    fs::create_dir_all(temporary.path().join("app/backend")).unwrap();
    fs::write(
        temporary.path().join("app/backend/lib.rs"),
        "pub fn app() {}\n",
    )
    .unwrap();
    let backend = SourceFingerprints::read(temporary.path()).unwrap();
    assert!(specification.changes(&backend).backend);

    fs::create_dir_all(temporary.path().join("app/web")).unwrap();
    fs::write(temporary.path().join("app/web/registry.ts"), "export {};\n").unwrap();
    let web = SourceFingerprints::read(temporary.path()).unwrap();
    assert!(backend.changes(&web).web);
}

#[test]
fn paths_map_to_the_narrowest_reload_scope() {
    let project = Path::new("/project");
    assert_eq!(
        classify_path(project, Path::new("/project/spec/main.yaml")),
        ProjectChanges {
            specification: true,
            backend: true,
            web: true,
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/project/app/backend/src/lib.rs")),
        ProjectChanges {
            backend: true,
            ..ProjectChanges::default()
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/project/app/web/registry.ts")),
        ProjectChanges {
            web: true,
            ..ProjectChanges::default()
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/project/modules/example/module.toml")),
        ProjectChanges {
            specification: true,
            backend: true,
            web: true,
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/project/.env")),
        ProjectChanges {
            specification: true,
            backend: true,
            web: true,
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/project/rust-toolchain.toml")),
        ProjectChanges {
            backend: true,
            ..ProjectChanges::default()
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/project/.cargo/config.toml")),
        ProjectChanges {
            backend: true,
            ..ProjectChanges::default()
        }
    );
    assert_eq!(
        classify_path(project, Path::new("/outside.yaml")),
        ProjectChanges::default()
    );
    let mut merged = ProjectChanges::default();
    merged.merge(ProjectChanges {
        web: true,
        ..ProjectChanges::default()
    });
    merged.merge(ProjectChanges {
        backend: true,
        ..ProjectChanges::default()
    });
    assert!(!merged.is_empty());
    assert!(merged.backend && merged.web);
}

#[test]
fn watcher_reports_web_changes_without_backend_reload() {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir_all(temporary.path().join("app/web")).unwrap();
    let mut watcher = ProjectWatcher::start(temporary.path()).unwrap();
    fs::write(temporary.path().join("app/web/registry.ts"), "export {};\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let changes = loop {
        if let Some(changes) = watcher.changes(Duration::from_millis(250)).unwrap() {
            break changes;
        }
        assert!(
            Instant::now() < deadline,
            "watcher did not report the change"
        );
    };
    assert_eq!(
        changes,
        ProjectChanges {
            web: true,
            ..ProjectChanges::default()
        }
    );
}

#[cfg(unix)]
#[test]
fn source_fingerprints_do_not_follow_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("modules")).unwrap();
    symlink(temporary.path(), temporary.path().join("modules/loop")).unwrap();
    assert!(SourceFingerprints::read(temporary.path()).is_ok());
}
