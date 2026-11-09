#[allow(dead_code)]
mod support;

use std::{fs, path::Path};

fn require_product_environment() {
    if std::env::var("APPSTRUCT_REQUIRE_PRODUCT_TESTS").as_deref() != Ok("1") {
        return;
    }
    for name in [
        "APPSTRUCT_E2E_DATABASE_URL",
        "APPSTRUCT_AUTH_MFA_KEY",
        "APPSTRUCT_REPORT_SNAPSHOT_KEY",
        "APPSTRUCT_STRIPE_WEBHOOK_SECRET",
        "APPSTRUCT_AI_ENDPOINT",
        "APPSTRUCT_AI_MODEL",
    ] {
        assert!(
            std::env::var(name).is_ok_and(|value| !value.is_empty()),
            "{name} is required by the product database gate"
        );
    }
    assert_eq!(
        std::env::var("APPSTRUCT_TEAM_SEAT_LIMIT").as_deref(),
        Ok("2")
    );
    assert_eq!(std::env::var("APPSTRUCT_AI_ENABLED").as_deref(), Ok("true"));
}

#[test]
fn generated_saas_enforces_allowances_and_refunds_failed_work() {
    require_product_environment();
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo");
    let ir = appstruct_compiler::compile_project(&example).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = temporary.path().join(artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = temporary.path().join("backend");
    let openapi: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temporary.path().join("openapi/openapi.json")).unwrap(),
    )
    .unwrap();
    assert!(openapi["paths"]["/api/admin/business"]["get"]["responses"]["403"].is_object());
    assert!(
        openapi["paths"]["/api/reports/templates/{name}/runs"]["post"]["responses"]["402"]
            .is_object()
    );
    assert!(
        openapi["paths"]["/api/files"]["post"]["requestBody"]["content"]["text/plain"].is_object()
    );
    assert!(
        openapi["paths"]["/api/product/team"]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|parameter| parameter["name"] == "X-AppStruct-Tenant"
                && parameter["required"] == true)
    );
    assert!(
        openapi["paths"]["/api/files"]["post"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|parameter| parameter["name"] == "X-CSRF-Token")
    );
    let manifest = backend.join("Cargo.toml");
    let source = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{source}\n[dev-dependencies]\ntower = {{ version = \"=0.5.3\", features = [\"util\"] }}\n")).unwrap();
    let library = backend.join("src/lib.rs");
    let source = fs::read_to_string(&library).unwrap();
    fs::write(library, format!("{source}\npub async fn run_product_job(database: database::Database, files: FileState) {{\nlet mail = MailState::from_env(database.clone()).unwrap();\nlet handler = jobs::generated_job_handler(database.clone(), mail, files, None);\nlet worker = JobWorker::new(database, handler);\nworker.run_once().await.unwrap();\n}}\n")).unwrap();
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(
        backend.join("tests/product.rs"),
        include_str!("product_journey/runtime.rs"),
    )
    .unwrap();
    fs::create_dir(backend.join("tests/product")).unwrap();
    fs::write(
        backend.join("tests/product/capabilities.rs"),
        include_str!("product_journey/capabilities.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&manifest, "product");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn app_owned_approvals_and_ai_preserve_product_invariants() {
    require_product_environment();
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo");
    let ir = appstruct_compiler::compile_project(&example).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = temporary
            .path()
            .join("generated")
            .join(artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let app = temporary.path().join("app/backend");
    fs::create_dir_all(app.join("src")).unwrap();
    fs::copy(
        example.join("app/backend/Cargo.toml"),
        app.join("Cargo.toml"),
    )
    .unwrap();
    for entry in fs::read_dir(example.join("app/backend/src")).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        fs::copy(entry.path(), app.join("src").join(entry.file_name())).unwrap();
    }
    fs::write(
        temporary.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"app/backend\"]\nresolver = \"3\"\n",
    )
    .unwrap();
    let output = support::cargo_test_lib(&app.join("Cargo.toml"));
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
