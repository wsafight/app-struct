use appstruct_compiler::{
    compile_project, expanded_preset, preset_info, project_lock, updated_project_lock,
};
use appstruct_ir::{FileProviderIr, MailProviderIr};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-preset-project")
}

#[test]
fn expands_and_locks_official_saas_preset() {
    let ir = compile_project(&fixture()).unwrap();
    let preset = ir.preset.as_ref().unwrap();
    assert_eq!(preset.name, "appstruct/saas");
    assert_eq!(preset.version, 1);
    assert_eq!(
        preset.digest,
        preset_info("appstruct/saas", 1).unwrap().digest
    );
    assert!(ir.auth.enabled);
    assert!(ir.tenant.enabled);
    assert!(ir.audit.enabled);
    assert_eq!(ir.mail.provider, MailProviderIr::Capture);
    assert_eq!(ir.jobs.queues.len(), 2);
    assert_eq!(ir.file.provider, FileProviderIr::Local);
    assert_eq!(ir.file.max_bytes, 10_485_760);
}

#[test]
fn user_module_values_override_preset_defaults() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "includes:\n",
        concat!(
            "modules:\n",
            "  auth:\n",
            "    registration: false\n",
            "  jobs:\n",
            "    poll_interval_ms: 500\n",
            "  file:\n",
            "    max_bytes: 2048\n\n",
            "includes:\n",
        ),
    );
    let ir = compile_project(temporary.path()).unwrap();
    assert!(!ir.auth.registration_enabled);
    assert!(ir.auth.password_reset_enabled);
    assert_eq!(ir.jobs.poll_interval_ms, 500);
    assert_eq!(ir.jobs.queues.len(), 2);
    assert_eq!(ir.file.max_bytes, 2048);
    assert_eq!(ir.file.allowed_content_types.len(), 4);
    let expanded = expanded_preset(temporary.path()).unwrap().unwrap();
    assert!(expanded.starts_with("modules:\n  audit:\n"));
    assert!(expanded.contains("registration: false"));
    assert!(expanded.contains("password_reset: true"));
    assert!(expanded.contains("poll_interval_ms: 500"));
    assert!(expanded.contains("max_bytes: 2048"));
}

#[test]
fn canonical_template_lock_matches_the_checked_in_saas_example() {
    let expected = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo/appstruct.lock"),
    )
    .unwrap();
    assert_eq!(
        project_lock("saas", Some(("appstruct/saas", 1))).unwrap(),
        expected
    );
    assert!(project_lock("saas", Some(("unknown", 1))).is_none());
}

#[test]
fn rejects_missing_or_tampered_preset_lock() {
    let temporary = copied_fixture();
    fs::remove_file(temporary.path().join("appstruct.lock")).unwrap();
    assert_diagnostic(temporary.path(), "AS3059");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.lock"),
        "sha256:7267c3a362b328d5b162f4536ac7115c72ab81cf9f3e17542a8d9b6eba7965c5",
        "sha256:0000",
    );
    assert_diagnostic(temporary.path(), "AS3060");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.lock"),
        "tenant = \"0.1.0\"\n",
        "",
    );
    assert_diagnostic(temporary.path(), "AS3061");
}

#[test]
fn rejects_unknown_preset_before_lowering() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "name: appstruct/saas",
        "name: appstruct/enterprise",
    );
    assert_diagnostic(temporary.path(), "AS3058");
}

#[test]
fn update_lock_repairs_contract_and_preserves_template_identity() {
    let temporary = copied_fixture();
    let lock_path = temporary.path().join("appstruct.lock");
    fs::write(
        &lock_path,
        concat!(
            "lock_version = 1\n",
            "appstruct = \"0.0.1\"\n\n",
            "[template]\n",
            "name = \"saas\"\n",
            "version = \"0.0.1\"\n\n",
            "[preset]\n",
            "name = \"appstruct/saas\"\n",
            "version = 1\n",
            "digest = \"sha256:stale\"\n",
        ),
    )
    .unwrap();

    assert_eq!(
        updated_project_lock(temporary.path()).unwrap(),
        project_lock("saas", Some(("appstruct/saas", 1))).unwrap()
    );
    assert!(fs::read_to_string(lock_path).unwrap().contains("stale"));
}

fn copied_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in [
        "appstruct.yaml",
        "appstruct.lock",
        "spec/identity.yaml",
        "spec/project.yaml",
    ] {
        fs::copy(fixture().join(relative), temporary.path().join(relative)).unwrap();
    }
    temporary
}

fn replace(path: &Path, old: &str, new: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains(old));
    fs::write(path, source.replacen(old, new, 1)).unwrap();
}

fn assert_diagnostic(project: &Path, code: &str) {
    let diagnostics = compile_project(project).unwrap_err();
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "expected {code}, got {diagnostics:#?}"
    );
}
