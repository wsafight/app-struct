use appstruct_migrate::{connect_database, extract, initial_migration};
use serde_json::Value;
use std::fs;
use std::process::{Command, Output};

fn project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("appstruct.yaml"), "version: 1\napp: { name: baseline-demo }\ndatabase: { provider: postgres }\nincludes: [domain.yaml]\n").unwrap();
    fs::write(
        project.path().join("domain.yaml"),
        r"domain: core
entities:
  Note:
    fields:
      id: { type: uuid, primary_key: true }
      title: { type: string, required: true }
    access:
      list: { public: true }
      read: { public: true }
      create: { public: true }
      update: { public: true }
      delete: { public: true }
",
    )
    .unwrap();
    project
}

fn run(project: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(project)
        .args(args)
        .args(["--format", "json"])
        .env_remove("DATABASE_URL")
        .output()
        .unwrap()
}

#[test]
fn baseline_requires_database_configuration_and_does_not_write_files() {
    let project = project();
    let output = run(project.path(), &["migrate", "baseline"]);
    assert_eq!(output.status.code(), Some(3));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert!(!project.path().join("migrations").exists());
    assert!(!project.path().join(".appstruct").exists());
}

#[test]
fn baseline_cli_previews_and_registers_using_project_dotenv() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else {
        return;
    };
    let schema_name = format!("baseline_cli_{}", std::process::id());
    let mut client = connect_database(&url).unwrap();
    client
        .batch_execute(&format!(
            "CREATE SCHEMA {schema_name}; SET search_path TO {schema_name}"
        ))
        .unwrap();
    let guard = SchemaGuard {
        client,
        schema: schema_name.clone(),
    };
    let project = project();
    let ir = appstruct_compiler::compile_project(project.path()).unwrap();
    let schema = extract(&ir).unwrap();
    let mut guard = guard;
    guard
        .client
        .batch_execute(&initial_migration(&schema))
        .unwrap();
    guard.client.batch_execute("INSERT INTO notes (id, title) VALUES ('00000000-0000-4000-8000-000000000001', 'keep me')").unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    fs::write(
        project.path().join(".env"),
        format!("DATABASE_URL='{url}{separator}options=-csearch_path%3D{schema_name}'\n"),
    )
    .unwrap();

    let preview = run(project.path(), &["migrate", "baseline"]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stdout)
    );
    let report: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(report["result"]["registered"], false);
    assert_eq!(report["result"]["accepted"], false);
    assert!(!project.path().join("migrations").exists());
    assert!(!project.path().join(".appstruct").exists());
    for _ in 0..2 {
        let output = run(project.path(), &["migrate", "baseline", "--accept"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["result"]["registered"], true);
    }
    assert_eq!(
        fs::read_dir(project.path().join("migrations"))
            .unwrap()
            .count(),
        1
    );
    let status = run(project.path(), &["migrate", "status"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
    let report: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(report["result"]["applied"], 1);
    assert_eq!(report["result"]["pending"], 0);
    assert_eq!(
        guard
            .client
            .query_one("SELECT title FROM notes", &[])
            .unwrap()
            .get::<_, String>(0),
        "keep me"
    );
}

struct SchemaGuard {
    client: postgres::Client,
    schema: String,
}

impl Drop for SchemaGuard {
    fn drop(&mut self) {
        let _ = self
            .client
            .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.schema));
    }
}
