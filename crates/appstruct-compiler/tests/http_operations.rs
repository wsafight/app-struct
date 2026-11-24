use appstruct_compiler::compile_project;
use std::fs;
use tempfile::tempdir;

fn write_project(domain_yaml: &str) -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("appstruct.yaml"),
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
    fs::write(dir.path().join("domain.yaml"), domain_yaml).unwrap();
    dir
}

#[test]
fn lowers_declarative_http_command_and_query_with_defaults_and_overrides() {
    let project = write_project(
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
    );

    let ir = compile_project(project.path()).expect("spec compiles");
    assert_eq!(ir.commands.len(), 1);
    let http = ir.commands[0].http.as_ref().expect("command http lowered");
    assert_eq!(http.url_env, "LEAD_SCORING_URL");
    assert_eq!(http.secret_env.as_deref(), Some("LEAD_SCORING_SECRET"));
    assert_eq!(http.timeout_ms, 5000);
    assert_eq!(http.retries, 2);

    assert_eq!(ir.queries.len(), 1);
    let q_http = ir.queries[0].http.as_ref().expect("query http lowered");
    assert_eq!(q_http.url_env, "LEAD_HEALTH_URL");
    assert_eq!(q_http.secret_env, None);
    assert_eq!(q_http.timeout_ms, 10_000);
    assert_eq!(q_http.retries, 0);
}

#[test]
fn rejects_invalid_http_operation_env_timeout_and_retries() {
    let project = write_project(
        r"
domain: crm
entities:
  Lead:
    fields:
      id: { type: uuid, primary_key: true }
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

commands:
  ScoreLead:
    input: ScoreInput
    output: ScoreInput
    access: { public: true }
    http:
      url_env: lowercase_bad
      secret_env: 123_BAD
      timeout_ms: 50
      retries: 9
",
    );

    let errors = compile_project(project.path()).unwrap_err();
    let messages: Vec<&str> = errors.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("http.url_env")));
    assert!(messages.iter().any(|m| m.contains("http.secret_env")));
    assert!(messages.iter().any(|m| m.contains("http.timeout_ms")));
    assert!(messages.iter().any(|m| m.contains("http.retries")));
}
