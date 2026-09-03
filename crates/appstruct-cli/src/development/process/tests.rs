use super::*;
use crate::environment::ProjectEnvironment;
use std::fs;
use std::io::Cursor;
use std::process::Stdio;

#[test]
fn external_managed_database_is_a_noop() {
    let mut database = ManagedDatabase::external();
    database.update_environment(ProjectEnvironment::default());
    database.stop().unwrap();
}

#[test]
fn log_pipe_ignores_missing_handles_and_drains_available_lines() {
    assert!(log_pipe("api", None::<Cursor<Vec<u8>>>).is_none());
    let handle = log_pipe("api", Some(Cursor::new(b"ready\n".to_vec()))).unwrap();
    handle.join().unwrap();
}

#[test]
fn terminate_stops_a_sleeping_child() {
    let mut child = Command::new("sleep")
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    terminate(&mut child);
    assert!(child.try_wait().unwrap().is_some());
}

#[test]
fn failure_reports_an_exited_api_process() {
    let api = Command::new("true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let web = Command::new("sleep")
        .arg("30")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut processes = DevProcesses {
        api,
        web,
        logs: Vec::new(),
    };
    let mut failure = None;
    for _ in 0..50 {
        if let Some(message) = processes.failure().unwrap() {
            failure = Some(message);
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("API")),
        "{failure:?}"
    );
    processes.stop();
}

#[test]
fn start_api_fails_when_the_generated_binary_is_missing() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("appstruct.lock"),
        "lock_version = 1\nproject_layout_version = 2\nappstruct = \"0.1.0\"\n",
    )
    .unwrap();
    let error = start_api(
        project.path(),
        &ProjectEnvironment::default(),
        "postgresql://127.0.0.1:1/appstruct?sslmode=disable",
        3000,
        "http://127.0.0.1:5173",
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}
