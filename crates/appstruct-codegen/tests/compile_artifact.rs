use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use appstruct_ir::Cardinality;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn generated_fixture_is_a_compilable_rust_crate() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    assert_m2_contract(&artifacts);
    let temporary = tempfile::tempdir().unwrap();

    for artifact in artifacts {
        let destination = temporary
            .path()
            .join("generated")
            .join(artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, artifact.content).unwrap();
    }

    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    let status = Command::new("cargo")
        .args(["check", "--quiet", "--manifest-path"])
        .arg(manifest)
        .env("CARGO_TARGET_DIR", temporary.path().join("target"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn one_to_one_relation_generates_has_one_inverse() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.relations[0].cardinality = Cardinality::OneToOne;
    let artifacts = plan(&ir).unwrap();

    assert!(
        artifact_text(&artifacts, "backend/src/entities/project.rs")
            .contains("pub task: HasOne<super::task::Entity>")
    );
}

#[test]
fn m3_extensions_require_every_handler_at_compile_time() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m3-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let extensions =
        fs::read_to_string(temporary.path().join("generated/backend/src/extensions.rs")).unwrap();
    assert!(extensions.contains("pub trait ArchiveProjectHandler"));
    assert!(extensions.contains("pub trait ProjectMetricsHandler"));
    assert!(extensions.contains("pub trait ProjectHooks"));
    assert!(extensions.contains("pub trait ProjectPolicy"));
    let project_api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(project_api.contains("after_commit hook failed"));
    assert!(!project_api.contains("HookOperation::Create, &model).await?"));
    assert!(artifact_text(&artifacts, "web/src/main.tsx").contains("../../../app/web/registry"));
    assert!(
        artifact_text(&artifacts, "web/src/generated/client.ts").contains("archiveProjectCommand")
    );
    assert!(
        artifact_text(&artifacts, "web/src/generated/registry.ts")
            .contains("ProjectMetadataEditor")
    );
    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/commands/archive-project"]["post"].is_object());
    assert!(openapi["paths"]["/api/queries/project-metrics"]["get"].is_object());

    let generated_manifest = temporary.path().join("generated/backend/Cargo.toml");
    assert!(cargo_check(&generated_manifest, true).status.success());

    let server = temporary.path().join("server");
    fs::create_dir_all(server.join("src")).unwrap();
    fs::write(server.join("Cargo.toml"), server_manifest()).unwrap();
    fs::write(server.join("src/main.rs"), missing_handler_source()).unwrap();
    let missing = Command::new("cargo")
        .args(["check", "--quiet", "--manifest-path"])
        .arg(server.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", temporary.path().join("server-target"))
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("ProjectMetricsHandler"));

    fs::write(server.join("src/main.rs"), complete_handler_source()).unwrap();
    let complete = Command::new("cargo")
        .args(["check", "--quiet", "--manifest-path"])
        .arg(server.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", temporary.path().join("server-target"))
        .status()
        .unwrap();
    assert!(complete.success());
}

#[test]
fn m4_auth_and_owner_scope_generate_a_compilable_backend() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    assert_eq!(artifacts.len(), 40);
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_auth_accounts"));
    assert!(sql.contains("_appstruct_auth_sessions"));
    assert!(sql.contains("_appstruct_auth_password_resets"));
    let project_api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(project_api.contains("actor.has_role(\"admin\")"));
    assert!(project_api.contains("Column::OwnerId.eq(actor.id)"));
    assert!(artifact_text(&artifacts, "backend/src/auth/handlers.rs").contains("Argon2"));
    assert_m4_openapi_contract(&artifacts);

    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    let checked = cargo_check(&manifest, true);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn m4_disabled_auth_flows_are_not_published() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.auth.registration_enabled = false;
    ir.auth.password_reset_enabled = false;
    let artifacts = plan(&ir).unwrap();
    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();

    assert!(openapi["paths"]["/api/auth/register"].is_null());
    assert!(openapi["paths"]["/api/auth/password/request"].is_null());
    assert!(openapi["paths"]["/api/auth/password/reset"].is_null());
    assert!(
        artifact_text(&artifacts, "backend/src/auth/session.rs").contains("DisabledMailSender")
    );
    assert!(
        artifact_text(&artifacts, "web/src/auth/AuthPages.tsx")
            .contains("if (!authFeatures.passwordReset)")
    );
}

fn assert_m4_openapi_contract(artifacts: &[Artifact]) {
    let openapi: Value =
        serde_json::from_str(artifact_text(artifacts, "openapi/openapi.json")).unwrap();
    assert_eq!(
        openapi["components"]["securitySchemes"]["cookieSession"]["name"],
        "appstruct_session"
    );
    assert_eq!(
        openapi["paths"]["/api/projects/"]["get"]["security"][0]["cookieSession"],
        serde_json::json!([])
    );
    assert!(openapi["paths"]["/api/auth/register"]["post"].is_object());
    assert!(openapi["paths"]["/api/auth/password/request"]["post"].is_object());
    assert_eq!(
        openapi["paths"]["/api/auth/logout"]["post"]["parameters"][0]["name"],
        "X-CSRF-Token"
    );
    assert_eq!(
        openapi["components"]["schemas"]["AuthUser"]["properties"]["roles"]["items"]["enum"],
        serde_json::json!(["admin", "member"])
    );
}

fn assert_m2_contract(artifacts: &[Artifact]) {
    assert_eq!(artifacts.len(), 34);
    assert!(artifact_text(artifacts, "database/0001_initial.sql").contains("CREATE TABLE"));
    assert!(artifact_text(artifacts, "backend/src/lib.rs").contains("/health/ready"));
    assert!(artifact_text(artifacts, "backend/src/lib.rs").contains("MakeRequestUuid"));
    assert!(artifact_text(artifacts, "web/pnpm-lock.yaml").contains("lockfileVersion"));
    assert!(artifact_text(artifacts, "web/src/generated/client.ts").contains("ListResponse"));
    assert!(artifact_text(artifacts, "web/src/generated/client.ts").contains("range_filters"));
    assert!(artifact_text(artifacts, "web/src/generated/client.ts").contains("resourceEtags"));
    assert!(artifact_text(artifacts, "web/src/generated/client.ts").contains("If-Match"));
    assert!(
        artifact_text(artifacts, "web/src/generated/resources.ts")
            .contains("minimum: \"0\", maximum: \"5\"")
    );
    assert!(
        artifact_text(artifacts, "backend/src/entities/project.rs")
            .contains("pub tasks: HasMany<super::task::Entity>")
    );
    assert!(
        artifact_text(artifacts, "backend/src/entities/task.rs")
            .contains("pub project: BelongsTo<super::project::Entity>")
    );

    let schema: Value =
        serde_json::from_str(artifact_text(artifacts, "database/schema.json")).unwrap();
    assert_eq!(schema["tables"][0]["name"], "projects");
    assert_eq!(schema["tables"][1]["name"], "tasks");
    assert_eq!(schema["foreign_keys"][0]["source_column"], "project_id");

    let openapi: Value =
        serde_json::from_str(artifact_text(artifacts, "openapi/openapi.json")).unwrap();
    assert_eq!(
        openapi["components"]["securitySchemes"],
        serde_json::json!({})
    );
    assert_eq!(
        openapi["paths"]["/api/projects/"]["get"]["security"],
        serde_json::json!([])
    );
    assert!(openapi["paths"]["/api/projects/"]["post"].is_object());
    assert!(openapi["paths"]["/api/projects/{id}"]["patch"].is_object());
    assert!(
        openapi["paths"]["/api/projects/{id}"]["get"]["responses"]["200"]["headers"]["ETag"]
            .is_object()
    );
    assert_eq!(
        openapi["paths"]["/api/projects/{id}"]["patch"]["parameters"][0]["name"],
        "If-Match"
    );
    assert!(openapi["paths"]["/api/projects/{id}"]["patch"]["responses"]["412"].is_object());
    assert!(openapi["paths"]["/api/projects/{id}"]["delete"]["responses"]["428"].is_object());
    assert_eq!(
        openapi["components"]["schemas"]["ProjectListResponse"]["properties"]["meta"]["type"],
        "object"
    );
}

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

fn write_artifacts(root: &Path, artifacts: &[Artifact]) {
    for artifact in artifacts {
        let destination = root.join("generated").join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &artifact.content).unwrap();
    }
}

fn cargo_check(manifest: &Path, library_only: bool) -> std::process::Output {
    let mut command = Command::new("cargo");
    command
        .args(["check", "--quiet", "--manifest-path"])
        .arg(manifest);
    if library_only {
        command.arg("--lib");
    }
    command
        .env(
            "CARGO_TARGET_DIR",
            manifest.parent().unwrap().join("target"),
        )
        .output()
        .unwrap()
}

fn server_manifest() -> &'static str {
    r#"[package]
name = "appstruct-extension-server"
version = "0.0.0"
edition = "2024"

[dependencies]
appstruct-generated-backend = { path = "../generated/backend" }
async-trait = "0.1.89"
"#
}

fn missing_handler_source() -> &'static str {
    r"use appstruct_generated_backend::{ApiError, AppExtensions, RequestContext};
use appstruct_generated_backend::entities::project;
use appstruct_generated_backend::extensions::{ArchiveProjectHandler, ArchiveProjectInput};
use async_trait::async_trait;

struct Handlers;

#[async_trait]
impl ArchiveProjectHandler for Handlers {
    async fn execute(&self, _ctx: &RequestContext, _input: ArchiveProjectInput) -> Result<project::Model, ApiError> {
        Err(ApiError::NotFound)
    }
}

fn main() { let _extensions = AppExtensions::builder().handlers(Handlers).build(); }
"
}

fn complete_handler_source() -> &'static str {
    r"use appstruct_generated_backend::{ApiError, AppExtensions, RequestContext};
use appstruct_generated_backend::entities::project;
use appstruct_generated_backend::extensions::{ArchiveProjectHandler, ArchiveProjectInput, ProjectMetrics, ProjectMetricsHandler};
use async_trait::async_trait;

struct Handlers;

#[async_trait]
impl ArchiveProjectHandler for Handlers {
    async fn execute(&self, _ctx: &RequestContext, _input: ArchiveProjectInput) -> Result<project::Model, ApiError> {
        Err(ApiError::NotFound)
    }
}

#[async_trait]
impl ProjectMetricsHandler for Handlers {
    async fn execute(&self, _ctx: &RequestContext) -> Result<ProjectMetrics, ApiError> {
        Ok(ProjectMetrics { active: 0, total: 0 })
    }
}

fn main() { let _extensions = AppExtensions::builder().handlers(Handlers).build(); }
"
}
