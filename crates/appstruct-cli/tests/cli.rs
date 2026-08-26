use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn check_emits_machine_readable_diagnostics_contract() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .args([
            "--project",
            fixture.to_str().unwrap(),
            "check",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], true);
    assert_eq!(report["entity_count"], 2);
    assert_eq!(report["diagnostics"], serde_json::json!([]));
}

#[test]
fn project_discovery_failure_honors_json_format() {
    let temporary = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .args([
            "--project",
            temporary.path().to_str().unwrap(),
            "check",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["diagnostics"][0]["code"], "AS1008");
}

#[test]
fn migration_dev_accepts_safe_addition_and_blocks_table_deletion() {
    let project = temporary_project("m2-project");
    let initial = run(&project, &["migrate", "dev", "--accept"]);
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let snapshot = project.join(".appstruct/schema.snapshot.json");
    let initial_snapshot = fs::read(&snapshot).unwrap();
    let initial_migration =
        fs::read_to_string(project.join("migrations/0001_appstruct.sql")).unwrap();
    assert!(initial_migration.contains("-- appstruct:schema-sha256="));

    let spec_path = project.join("spec/project.yaml");
    let spec = fs::read_to_string(&spec_path).unwrap();
    let with_notes = spec.replacen(
        "      created_at:\n",
        "      notes:\n        type: text\n      created_at:\n",
        1,
    );
    fs::write(&spec_path, with_notes).unwrap();
    let safe = run(&project, &["migrate", "dev", "--accept"]);
    assert!(
        safe.status.success(),
        "{}",
        String::from_utf8_lossy(&safe.stderr)
    );
    let safe_snapshot = fs::read(&snapshot).unwrap();
    assert_ne!(safe_snapshot, initial_snapshot);

    let spec = fs::read_to_string(&spec_path).unwrap();
    let without_task = spec.split_once("\n  Task:\n").unwrap().0;
    fs::write(&spec_path, format!("{without_task}\n")).unwrap();
    let blocked = run(&project, &["migrate", "dev", "--accept"]);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("AS4102"));
    assert_eq!(fs::read(snapshot).unwrap(), safe_snapshot);
    assert_eq!(migration_count(&project), 2);
}

#[test]
fn database_migration_commands_require_database_url() {
    let project = temporary_project("m2-project");
    for command in ["apply", "status"] {
        let output = run(&project, &["migrate", command]);
        assert_eq!(output.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&output.stderr).contains("AS4107"));
    }
}

#[test]
fn new_creates_valid_minimal_and_dashboard_projects_without_overwrite() {
    let temporary = tempfile::tempdir().unwrap();
    for (name, template) in [("notes-app", "minimal"), ("project-app", "dashboard")] {
        let created = run_new(temporary.path(), name, template);
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        let project = temporary.path().join(name);
        assert!(run(&project, &["check"]).status.success());
        assert!(project.join("appstruct.lock").is_file());
        assert!(project.join("rust-toolchain.toml").is_file());

        let readme = fs::read(project.join("README.md")).unwrap();
        let repeated = run_new(temporary.path(), name, template);
        assert_eq!(repeated.status.code(), Some(1));
        assert_eq!(fs::read(project.join("README.md")).unwrap(), readme);
    }
    assert!(temporary.path().join("project-app/compose.yaml").is_file());
    assert!(!temporary.path().join("notes-app/compose.yaml").exists());
}

#[test]
fn doctor_json_reports_missing_external_database_configuration() {
    let project = temporary_project("m2-project");
    let output = run(&project, &["doctor", "--format", "json"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["healthy"], false);
    assert!(report["checks"].as_array().unwrap().iter().any(|check| {
        check["name"] == "PostgreSQL"
            && check["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("DATABASE_URL"))
    }));
}

#[test]
fn generation_manifest_blocks_modified_and_unknown_files() {
    let project = temporary_project("m2-project");
    let initial = run(&project, &["generate"]);
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let manifest_path = project.join("generated/.appstruct-manifest.json");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["manifest_version"], 1);
    assert_eq!(manifest["artifacts"].as_array().unwrap().len(), 36);

    let cargo_lock = project.join("generated/backend/Cargo.lock");
    fs::write(&cargo_lock, "# build-generated lockfile\n").unwrap();
    let second = run(&project, &["generate"]);
    assert!(second.status.success());
    assert!(String::from_utf8_lossy(&second.stdout).contains("0 changed"));
    assert_eq!(
        fs::read_to_string(&cargo_lock).unwrap(),
        "# build-generated lockfile\n"
    );
    assert!(run(&project, &["generate", "--check"]).status.success());

    let owned = project.join("generated/openapi/openapi.json");
    fs::write(&owned, "manually changed\n").unwrap();
    let modified = run(&project, &["generate"]);
    assert!(!modified.status.success());
    assert!(String::from_utf8_lossy(&modified.stderr).contains("was modified outside AppStruct"));
    assert_eq!(fs::read_to_string(owned).unwrap(), "manually changed\n");

    let other = temporary_project("m2-project");
    assert!(run(&other, &["generate"]).status.success());
    fs::write(other.join("generated/user-code.rs"), "fn user_code() {}\n").unwrap();
    let unknown = run(&other, &["generate"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown file"));
}

#[test]
fn generation_never_overwrites_user_extension_directories() {
    let project = temporary_project("m2-project");
    let registry = project.join("app/web/registry.ts");
    fs::create_dir_all(registry.parent().unwrap()).unwrap();
    fs::write(&registry, "export const userRegistry = true;\n").unwrap();

    assert!(run(&project, &["generate"]).status.success());
    let spec_path = project.join("spec/project.yaml");
    let spec = fs::read_to_string(&spec_path).unwrap();
    fs::write(
        &spec_path,
        spec.replacen(
            "      created_at:\n",
            "      notes:\n        type: text\n      created_at:\n",
            1,
        ),
    )
    .unwrap();
    assert!(run(&project, &["generate"]).status.success());
    assert_eq!(
        fs::read_to_string(registry).unwrap(),
        "export const userRegistry = true;\n"
    );
}

#[test]
fn generation_is_byte_deterministic_across_project_directories() {
    let first = temporary_project("m2-project");
    let second = temporary_project("m2-project");
    assert!(run(&first, &["generate"]).status.success());
    assert!(run(&second, &["generate"]).status.success());
    assert_directories_equal(&first.join("generated"), &second.join("generated"));
}

fn temporary_project(fixture: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(fixture);
    let destination = tempfile::tempdir().unwrap().keep();
    copy_directory(&source, &destination);
    destination
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn run(project: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(project)
        .args(arguments)
        .env_remove("DATABASE_URL")
        .output()
        .unwrap()
}

fn run_new(parent: &Path, name: &str, template: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .current_dir(parent)
        .args(["new", name, "--template", template])
        .output()
        .unwrap()
}

fn migration_count(project: &Path) -> usize {
    fs::read_dir(project.join("migrations"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|value| value == "sql"))
        .count()
}

fn assert_directories_equal(first: &Path, second: &Path) {
    let mut first_entries = fs::read_dir(first)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    let mut second_entries = fs::read_dir(second)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    first_entries.sort();
    second_entries.sort();
    assert_eq!(first_entries, second_entries, "directory entries differ");
    for name in first_entries {
        let first_path = first.join(&name);
        let second_path = second.join(name);
        if first_path.is_dir() {
            assert!(second_path.is_dir());
            assert_directories_equal(&first_path, &second_path);
        } else {
            assert_eq!(
                fs::read(&first_path).unwrap(),
                fs::read(&second_path).unwrap(),
                "file bytes differ for {}",
                first_path.display()
            );
        }
    }
}
