mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use appstruct_ir::Cardinality;
use serde_json::Value;
use std::fs;
use std::path::Path;
use support::{
    assert_rustfmt, cargo_check, complete_handler_source, missing_handler_source,
    prepare_generated_package, server_manifest,
};

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
    let app_backend = temporary.path().join("app/backend");
    fs::create_dir_all(app_backend.join("src")).unwrap();
    fs::write(
        app_backend.join("Cargo.toml"),
        concat!(
            "[package]\nname = \"appstruct-app-backend\"\nversion = \"0.0.0\"\n",
            "edition = \"2024\"\n\n[dependencies]\n",
            "appstruct-generated-backend = { path = \"../../generated/backend\" }\n",
        ),
    )
    .unwrap();
    fs::write(
        app_backend.join("src/lib.rs"),
        concat!(
            "use appstruct_generated_backend::AppExtensions;\n",
            "pub fn extensions() -> AppExtensions { AppExtensions::builder().build() }\n",
        ),
    )
    .unwrap();
    let server_manifest = temporary.path().join("generated/server/Cargo.toml");
    assert_rustfmt(&server_manifest);
    let server_checked = cargo_check(&server_manifest, false);
    assert!(
        server_checked.status.success(),
        "{}",
        String::from_utf8_lossy(&server_checked.stderr)
    );
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, false);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
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
    assert_rustfmt(&generated_manifest);
    let generated_package = prepare_generated_package(&generated_manifest).unwrap();
    assert!(cargo_check(&generated_manifest, true).status.success());

    let server = temporary.path().join("server");
    fs::create_dir_all(server.join("src")).unwrap();
    fs::write(
        server.join("Cargo.toml"),
        server_manifest(&generated_package),
    )
    .unwrap();
    fs::write(server.join("src/main.rs"), missing_handler_source()).unwrap();
    let missing = cargo_check(&server.join("Cargo.toml"), false);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("ProjectMetricsHandler"));

    fs::write(server.join("src/main.rs"), complete_handler_source()).unwrap();
    let complete = cargo_check(&server.join("Cargo.toml"), false);
    assert!(complete.status.success());
}

#[test]
fn m4_auth_and_owner_scope_generate_a_compilable_backend() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    assert_eq!(artifacts.len(), 50);
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
    let resources = artifact_text(&artifacts, "web/src/generated/resources.ts");
    assert!(resources.contains(r#""mode":"role","role":"admin""#));
    assert!(resources.contains("export const auditAccess"));
    let access = artifact_text(&artifacts, "web/src/resource.ts");
    assert!(access.contains("canAccessResource"));
    assert!(access.contains("`${logicalField}_id`"));
    assert_m4_openapi_contract(&artifacts);

    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest);
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

#[test]
fn m6_tenant_contract_generates_a_compilable_backend() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_tenant_organizations"));
    assert!(sql.contains("_appstruct_tenant_memberships"));
    assert!(sql.contains("PRIMARY KEY (\"organization_id\", \"user_id\")"));
    assert!(sql.contains("FOREIGN KEY (\"tenant_id\")"));
    assert!(sql.contains("UNIQUE (\"tenant_id\", \"id\")"));
    assert!(sql.contains(
        "FOREIGN KEY (\"tenant_id\", \"project_id\") REFERENCES \"projects\" (\"tenant_id\", \"id\")"
    ));

    let api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(api.contains("Column::TenantId.eq(context.require_tenant()?)"));
    assert!(api.contains("tenant_id: Set(context.require_tenant()?)"));
    assert!(!api.contains("pub tenant_id: uuid::Uuid"));

    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("X-AppStruct-Tenant"));
    assert!(client.contains("export const tenantApi"));
    assert!(
        artifacts
            .iter()
            .any(|artifact| { artifact.relative_path == Path::new("web/src/tenant/Tenant.tsx") })
    );

    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert_eq!(
        openapi["paths"]["/api/projects/"]["get"]["parameters"][4]["name"],
        "X-AppStruct-Tenant"
    );
    assert!(openapi["paths"]["/api/tenant/organizations"]["post"].is_object());

    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, true);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
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
    assert_eq!(artifacts.len(), 44);
    assert!(
        artifact_text(artifacts, "backend/Cargo.toml")
            .contains("appstruct-runtime = { path = \"runtime\" }")
    );
    assert!(
        artifact_text(artifacts, "backend/runtime/src/lib.rs").contains("pub trait ServiceHandle")
    );
    assert!(
        artifact_text(artifacts, "backend/runtime/src/lifecycle.rs")
            .contains("pub struct ModulePlan")
    );
    let backend = artifact_text(artifacts, "backend/src/lib.rs");
    assert!(backend.contains("pub const GENERATED_RUNTIME_API_VERSION: u32 = 1"));
    assert!(backend.contains("startup_plan().start(&mut context).await?"));
    assert!(
        artifact_text(artifacts, "server/Cargo.toml")
            .contains("appstruct-app-backend = { path = \"../../app/backend\" }")
    );
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
    assert_eq!(
        schema["foreign_keys"][0]["source_columns"],
        serde_json::json!(["project_id"])
    );

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
