use super::*;

#[test]
fn rejects_invalid_ports_before_project_access() {
    let missing = Path::new("/path/that/does/not/exist");
    assert_eq!(run(missing, 0, 5173), ExitCode::from(2));
    assert_eq!(run(missing, 3000, 3000), ExitCode::from(2));
}

#[test]
fn check_stopping_and_compile_surface_interrupt_and_invalid_projects() {
    let stopping = Arc::new(AtomicBool::new(true));
    let interrupted = check_stopping(&stopping).unwrap_err();
    assert_eq!(interrupted.kind(), io::ErrorKind::Interrupted);

    stopping.store(false, Ordering::SeqCst);
    check_stopping(&stopping).unwrap();

    let error = compile(Path::new("/missing-appstruct-project")).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn wait_for_database_honors_the_stop_signal() {
    let stopping = Arc::new(AtomicBool::new(true));
    let error = wait_for_database(
        "postgresql://appstruct:secret@127.0.0.1:1/appstruct?sslmode=disable",
        &stopping,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn status_reports_failed_commands() {
    let error = status(Command::new("false"), "build generated backend").unwrap_err();
    assert!(error.to_string().contains("build generated backend"));
}

#[test]
fn prepare_stops_immediately_when_signaled() {
    let stopping = AtomicBool::new(true);
    let error = prepare(
        Path::new("/missing"),
        "postgresql://127.0.0.1:1/appstruct?sslmode=disable",
        &ProjectEnvironment::load(Path::new("/missing")).unwrap_or_default(),
        DatabaseMigrationPolicy::Unmanaged,
        true,
        &stopping,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn run_reports_missing_projects_after_installing_the_signal_handler() {
    let missing = Path::new("/missing-appstruct-project");
    assert_ne!(run(missing, 3000, 5173), ExitCode::SUCCESS);
    assert_ne!(run(missing, 3001, 5174), ExitCode::SUCCESS);
}

#[test]
fn compile_succeeds_for_the_m0_fixture_and_build_helpers_fail_without_outputs() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    compile(&fixture).unwrap();
    let project = tempfile::tempdir().unwrap();
    let environment = ProjectEnvironment::default();
    assert!(build_backend(project.path(), &environment).is_err());
    assert!(install_web(project.path(), &environment).is_err());
    let stopping = Arc::new(AtomicBool::new(true));
    let Err(error) = DevSession::start(project.path(), 3000, 5173, stopping) else {
        panic!("expected startup to stop immediately");
    };
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn start_fails_for_external_projects_without_database_url() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: demo\ndatabase:\n  provider: postgres\n  dev:\n    mode: external\n    migration: unmanaged\nincludes: []\n",
    )
    .unwrap();
    let Err(error) =
        DevSession::start(project.path(), 3000, 5173, Arc::new(AtomicBool::new(false)))
    else {
        panic!("expected missing DATABASE_URL");
    };
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}
