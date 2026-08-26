use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn generated_fixture_is_a_compilable_rust_crate() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m1-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    assert_m1_contract(&artifacts);
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

fn assert_m1_contract(artifacts: &[Artifact]) {
    assert_eq!(artifacts.len(), 27);
    assert!(artifact_text(artifacts, "database/0001_initial.sql").contains("CREATE TABLE"));
    assert!(artifact_text(artifacts, "web/pnpm-lock.yaml").contains("lockfileVersion"));

    let schema: Value =
        serde_json::from_str(artifact_text(artifacts, "database/schema.json")).unwrap();
    assert_eq!(schema["tables"][0]["name"], "projects");

    let openapi: Value =
        serde_json::from_str(artifact_text(artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/projects/"]["post"].is_object());
    assert!(openapi["paths"]["/api/projects/{id}"]["patch"].is_object());
}

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}
