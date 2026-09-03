#[path = "compile_artifact/query_contract.rs"]
mod query_contract;
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
fn resources_publish_bulk_and_csv_contracts() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(api.contains("/_bulk"));
    assert!(api.contains("/_export.csv"));
    assert!(api.contains("/_import.csv"));
    assert!(api.contains("expected_revisions"));
    assert!(api.contains("bulk_request_size_is_valid"));
    assert!(api.contains("MAX_CSV_IMPORT_ROWS"));
    assert!(api.contains("CSV_EXPORT_PAGE_SIZE"));
    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert_eq!(
        openapi["components"]["schemas"]["BulkDeleteInput"]["properties"]["ids"]["maxItems"],
        100
    );
    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("bulkUpdate"));
    assert!(client.contains("exportCsv"));
    assert!(client.contains("importCsv"));
}

#[test]
fn resource_lists_publish_saved_view_controls() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let saved_views = artifact_text(&artifacts, "web/src/pages/resource-list/SavedViews.tsx");
    assert!(saved_views.contains("appstruct.saved-views"));
    assert!(saved_views.contains("copyViewLink"));
    assert!(saved_views.contains("Saved views"));
}

#[test]
fn generated_web_uses_the_tanstack_runtime() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let package = artifact_text(&artifacts, "web/package.json");
    let main = artifact_text(&artifacts, "web/src/main.tsx");
    let navigation = artifact_text(&artifacts, "web/src/navigation.tsx");
    let framework_test = artifact_text(&artifacts, "web/src/framework.test.ts");
    let routes = artifact_text(&artifacts, "web/src/app/ResourceRoutes.tsx");
    let list = artifact_text(&artifacts, "web/src/pages/ResourceList.tsx");
    let filters = artifact_text(&artifacts, "web/src/pages/ResourceFilters.tsx");
    let form = artifact_text(&artifacts, "web/src/pages/ResourceForm.tsx");

    assert!(package.contains("@tanstack/react-query"));
    assert!(package.contains("@tanstack/react-router"));
    assert!(package.contains("@tanstack/react-table"));
    assert!(package.contains("@tanstack/react-form"));
    assert!(package.contains("typescript-eslint"));
    assert!(package.contains("vitest run"));
    assert!(!package.contains("react-router-dom"));
    assert!(main.contains("QueryClientProvider"));
    assert!(navigation.contains("createRuntimeRouter"));
    let controller = artifact_text(&artifacts, "web/src/controller.ts");
    assert!(controller.contains("useResourceListController"));
    assert!(controller.contains("useResourceDetailController"));
    assert!(controller.contains("useMutation"));
    assert!(framework_test.contains("validateResourceSearch"));
    assert!(framework_test.contains("shouldRetryQuery"));
    assert!(framework_test.contains("canAccessRule"));
    assert!(framework_test.contains("buildResourceFilterQuery"));
    assert!(framework_test.contains("supportsInlineEdit"));
    assert!(routes.contains("validateSearch: validateResourceSearch"));
    let table = artifact_text(&artifacts, "web/src/pages/resource-list/ResourceTable.tsx");
    assert!(list.contains("useRealtimeResource"));
    assert!(table.contains("useTable"));
    assert!(table.contains("InlineEditor"));
    let inline_editor = artifact_text(&artifacts, "web/src/pages/resource-list/InlineEditor.tsx");
    assert!(inline_editor.contains("supportsInlineEdit"));
    assert!(list.contains("expected_revisions"));
    assert!(list.contains("ResourceFilters"));
    assert!(filters.contains("buildResourceFilterQuery"));
    assert!(form.contains("useForm"));
    assert!(form.contains("buildValidationSchema"));
    assert!(list.contains("useResourceListController"));
    let html = artifact_text(&artifacts, "web/index.html");
    let layout = artifact_text(&artifacts, "web/src/app/Layout.tsx");
    assert!(html.contains("<title>Project Manager</title>"));
    assert!(layout.contains("<span>Project Manager</span>"));
    assert!(!html.contains("__APP_TITLE__"));
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
    assert!(extensions.contains("async fn can_list(&self, _ctx: &RequestContext)"));
    let project_api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(project_api.contains("after_commit hook failed"));
    assert!(project_api.contains("can_list(&context).await?"));
    assert!(!project_api.contains("HookOperation::Create, &model).await?"));
    assert!(artifact_text(&artifacts, "web/src/main.tsx").contains("../../../app/web/registry"));
    assert!(
        artifact_text(&artifacts, "web/src/generated/client.ts").contains("archiveProjectCommand")
    );
    assert!(
        artifact_text(&artifacts, "web/src/generated/registry.ts")
            .contains("ProjectMetadataEditor")
    );
    assert!(
        artifact_text(&artifacts, "web/src/generated/registry.ts")
            .contains("resources: readonly ResourceDefinition[]")
    );
    assert!(
        artifact_text(&artifacts, "web/src/app/App.tsx")
            .contains("<Component resources={resources} />")
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
    assert_eq!(artifacts.len(), 76);
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_auth_accounts"));
    assert!(sql.contains("_appstruct_auth_sessions"));
    assert!(sql.contains("_appstruct_auth_password_resets"));
    assert!(sql.contains("_appstruct_auth_email_verifications"));
    assert!(sql.contains("_appstruct_auth_api_tokens"));
    assert!(sql.contains("_appstruct_auth_login_attempts"));
    let project_api = artifact_text(&artifacts, "backend/src/api/project.rs");
    assert!(project_api.contains("actor.has_role(\"admin\")"));
    assert!(project_api.contains("Column::OwnerId.eq(actor.id)"));
    assert!(artifact_text(&artifacts, "backend/src/auth/handlers.rs").contains("Argon2"));
    assert!(artifact_text(&artifacts, "backend/src/auth/recovery.rs").contains("verify_email"));
    assert!(artifact_text(&artifacts, "backend/src/auth/admin.rs").contains("admin_overview"));
    let session = artifact_text(&artifacts, "backend/src/auth/session.rs");
    assert!(session.contains("Bearer "));
    assert!(session.contains("APPSTRUCT_ALLOWED_ORIGIN"));
    assert!(session.contains("is required when APPSTRUCT_ENV=production"));
    assert!(session.contains("validate_browser_origin"));
    assert!(session.contains("record_login_failure"));
    assert!(session.contains("clear_login_attempts"));
    assert!(session.contains(r#"DELETE FROM "_appstruct_auth_login_attempts""#));
    let handlers = artifact_text(&artifacts, "backend/src/auth/handlers.rs");
    assert!(handlers.contains("record_login_failure"));
    assert!(handlers.contains("clear_login_attempts"));
    let layout = artifact_text(&artifacts, "web/src/app/Layout.tsx");
    assert!(layout.contains("sidebar-account"));
    assert_eq!(layout.matches("aria-label=\"Sign out\"").count(), 2);
    assert!(layout.contains("<span>Project Hub</span>"));
    assert!(artifact_text(&artifacts, "web/src/styles.css").contains(".sidebar-account"));
    assert!(
        artifact_text(&artifacts, "web/src/auth/AuthPages.tsx")
            .contains("auth-brand\">Project Hub")
    );
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
    let oauth = artifact_text(&artifacts, "backend/src/auth/oauth.rs");
    assert!(!oauth.contains("start_oidc"));
    assert!(!artifact_text(&artifacts, "backend/Cargo.toml").contains("reqwest"));
}

#[test]
fn admin_surface_is_wired_for_auth_and_audit_without_tenant() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.audit.enabled = true;
    ir.audit.reader_roles = vec!["admin".to_owned()];
    let artifacts = plan(&ir).unwrap();

    let app = artifact_text(&artifacts, "web/src/app/App.tsx");
    assert!(app.contains("AdminPage"));
    assert!(app.contains("AdminUsersPage"));
    assert!(app.contains("path: \"/admin\""));
    assert!(app.contains("path: \"/admin/users\""));
    let layout = artifact_text(&artifacts, "web/src/app/Layout.tsx");
    assert!(layout.contains("to=\"/admin\""));
    let client = artifact_text(&artifacts, "web/src/generated/client.ts");
    assert!(client.contains("adminFeatures"));
    assert!(client.contains("listUsers"));
    assert!(client.contains("revokeUserSessions"));
    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/admin/users"]["get"].is_object());
    assert!(openapi["paths"]["/api/admin/users/{id}/revoke-sessions"]["post"].is_object());
}

#[test]
fn oauth_enabled_auth_publishes_oidc_contracts() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.auth.oauth_enabled = true;
    let artifacts = plan(&ir).unwrap();
    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_auth_oauth_accounts"));
    assert!(artifact_text(&artifacts, "backend/src/auth/oauth.rs").contains("start_oidc"));
    assert!(artifact_text(&artifacts, "web/src/generated/client.ts").contains("startOidc"));
    let openapi: Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/auth/oauth/oidc/start"]["get"].is_object());
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);
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
    assert!(sql.contains("_appstruct_tenant_invitations"));
    assert!(sql.contains("UNIQUE"));
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
    assert!(
        openapi["paths"]["/api/projects/"]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|parameter| parameter["name"] == "X-AppStruct-Tenant")
    );
    assert!(openapi["paths"]["/api/tenant/organizations"]["post"].is_object());
    assert!(openapi["paths"]["/api/tenant/invitations"]["post"].is_object());
    assert!(openapi["paths"]["/api/tenant/invitations/{token}/accept"]["post"].is_object());

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
    assert!(openapi["paths"]["/api/auth/email/verify"]["post"].is_object());
    assert!(openapi["paths"]["/api/auth/tokens"]["post"].is_object());
    assert!(openapi["paths"]["/api/admin/overview"]["get"].is_object());
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
    assert_eq!(artifacts.len(), 66);
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
    assert!(
        artifact_text(artifacts, "backend/runtime/src/resource.rs")
            .contains("pub struct ListQuery")
    );
    assert!(
        artifact_text(artifacts, "backend/runtime/src/query.rs").contains("like_contains_pattern")
    );
    assert!(
        artifact_text(artifacts, "backend/runtime/src/origin.rs")
            .contains("validate_browser_origin")
    );
    let backend = artifact_text(artifacts, "backend/src/lib.rs");
    assert!(backend.contains("pub async fn connect_database"));
    assert!(backend.contains("ConnectOptions"));
    assert!(backend.contains("APPSTRUCT_DB_MAX_CONNECTIONS"));
    assert!(artifact_text(artifacts, "backend/Cargo.toml").contains("tinyvec = \"=1.12.0\""));
    let auth = artifact_text(artifacts, "backend/src/auth.rs");
    assert!(auth.contains("CorsLayer::permissive()"));
    assert!(auth.contains("APPSTRUCT_ENV"));
    let main = artifact_text(artifacts, "backend/src/main.rs");
    assert!(main.contains("connect_database(database_url)"));
    assert!(backend.contains("pub const GENERATED_RUNTIME_API_VERSION: u32 = 4"));
    assert!(backend.contains("startup_plan().start(&mut context).await?"));
    assert!(backend.contains("state.health.is_ready() && state.database.ping().await.is_ok()"));
    assert!(backend.contains("mark_draining"));
    let drained_http = backend.find("server_result = match").unwrap();
    let stopped_modules = backend[drained_http..]
        .find("stop_modules(&mut runtime).await;")
        .map(|offset| offset + drained_http)
        .unwrap();
    assert!(drained_http < stopped_modules);
    assert!(
        artifact_text(artifacts, "server/Cargo.toml")
            .contains("appstruct-app-backend = { path = \"../../app/backend\" }")
    );
    assert!(artifact_text(artifacts, "database/0001_initial.sql").contains("CREATE TABLE"));
    assert!(artifact_text(artifacts, "backend/src/lib.rs").contains("/health/ready"));
    assert!(artifact_text(artifacts, "backend/src/lib.rs").contains("appstruct_health_ready"));
    assert!(artifact_text(artifacts, "backend/src/lib.rs").contains("MakeRequestUuid"));
    assert!(artifact_text(artifacts, "web/pnpm-lock.yaml").contains("lockfileVersion"));
    assert!(artifact_text(artifacts, "web/.gitignore").contains("node_modules/"));
    query_contract::assert_query_contract(artifacts);
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
