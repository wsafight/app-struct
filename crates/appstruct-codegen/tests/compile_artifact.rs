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

fn assert_m2_contract(artifacts: &[Artifact]) {
    assert_eq!(artifacts.len(), 30);
    assert!(artifact_text(artifacts, "database/0001_initial.sql").contains("CREATE TABLE"));
    assert!(artifact_text(artifacts, "web/pnpm-lock.yaml").contains("lockfileVersion"));
    assert!(artifact_text(artifacts, "web/src/generated/client.ts").contains("ListResponse"));
    assert!(artifact_text(artifacts, "web/src/generated/client.ts").contains("range_filters"));
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
    assert!(openapi["paths"]["/api/projects/"]["post"].is_object());
    assert!(openapi["paths"]["/api/projects/{id}"]["patch"].is_object());
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
