use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn http_backed_commands_and_queries_require_no_rust_handlers() {
    let project = tempdir().unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        r"
version: 1
app:
  name: http-ext-demo
database:
  provider: postgres
includes:
  - domain.yaml
",
    )
    .unwrap();
    fs::write(
        project.path().join("domain.yaml"),
        r"
domain: crm
entities:
  Lead:
    fields:
      id: { type: uuid, primary_key: true }
      email: { type: string, required: true }
    access:
      list: { public: true }
      read: { public: true }
      create: { public: true }
      update: { public: true }
      delete: { public: true }

value_objects:
  ScoreInput:
    fields:
      email: { type: string, required: true }
  ScoreOutput:
    fields:
      score: { type: integer, required: true }

commands:
  ScoreLead:
    input: ScoreInput
    output: ScoreOutput
    access: { public: true }
    http:
      url_env: LEAD_SCORING_URL
      secret_env: LEAD_SCORING_SECRET
      timeout_ms: 5000
      retries: 2

queries:
  ScoreHealth:
    output: ScoreOutput
    access: { public: true }
    http:
      url_env: LEAD_HEALTH_URL
",
    )
    .unwrap();

    let ir = compile_project(project.path()).expect("spec compiles");
    let artifacts = plan(&ir).expect("codegen succeeds");

    let text = |path: &str| {
        let artifact = artifacts
            .iter()
            .find(|artifact| artifact.relative_path == Path::new(path))
            .unwrap_or_else(|| panic!("missing artifact {path}"));
        std::str::from_utf8(&artifact.content).unwrap()
    };

    let operations = text("backend/src/operations.rs");
    assert!(operations.contains("invoke_http_operation"));
    assert!(operations.contains("LEAD_SCORING_URL"));
    assert!(operations.contains("LEAD_SCORING_SECRET"));
    assert!(operations.contains("x-appstruct-signature"));

    let extensions = text("backend/src/extensions.rs");
    assert!(extensions.contains("pub struct DefaultHandlers"));
    assert!(!extensions.contains("ScoreLeadHandler"));
    assert!(!extensions.contains("ScoreHealthHandler"));

    let manifest = text("backend/Cargo.toml");
    assert!(manifest.contains("reqwest ="));
    assert!(manifest.contains("ring ="));
}
