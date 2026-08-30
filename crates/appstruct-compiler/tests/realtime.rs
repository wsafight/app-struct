use appstruct_compiler::compile_project;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-jobs-project")
}

#[test]
fn lowers_realtime_presence_settings() {
    let realtime = compile_project(&fixture()).unwrap().realtime;
    assert!(realtime.enabled);
    assert_eq!(realtime.heartbeat_seconds, 5);
    assert_eq!(realtime.presence_ttl_seconds, 15);
}

#[test]
fn realtime_requires_auth_and_a_longer_ttl() {
    for (old, new, code) in [
        (
            "    enabled: true\n    user_entity: User",
            "    enabled: false\n    user_entity: User",
            "AS3080",
        ),
        (
            "presence_ttl_seconds: 15",
            "presence_ttl_seconds: 10",
            "AS3082",
        ),
        ("heartbeat_seconds: 5", "heartbeat_seconds: 15", "AS3083"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        copy_project(&fixture(), temporary.path());
        let path = temporary.path().join("appstruct.yaml");
        let source = fs::read_to_string(&path).unwrap();
        fs::write(path, source.replacen(old, new, 1)).unwrap();
        let diagnostics = compile_project(temporary.path()).unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == code));
    }
}

fn copy_project(source: &Path, destination: &Path) {
    fs::create_dir(destination.join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), destination.join(relative)).unwrap();
    }
}
