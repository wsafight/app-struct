#[allow(dead_code)]
mod support;

use appstruct_codegen::Artifact;
use appstruct_ir::AppIr;
use std::{fs, path::Path};

#[test]
fn relation_write_generators_share_target_read_validation() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let artifacts =
        appstruct_codegen::plan(&appstruct_compiler::compile_project(&fixture).unwrap()).unwrap();
    let api = artifact_text(&artifacts, "backend/src/api/task.rs");
    assert_write_calls(api);
    let required = compact(api);
    assert!(required.contains("ifletSome(id)=Some(model.project_id){"));
    assert!(
        required.contains("state.extensions.project_policy().can_read(context,&target).await?")
    );
    assert!(required.contains(".lock_shared().one(context).await?.ok_or(ApiError::NotFound)?"));

    let artifacts = appstruct_codegen::plan(&nullable_tenant_ir()).unwrap();
    let api = artifact_text(&artifacts, "backend/src/api/task.rs");
    assert_write_calls(api);
    let tenant = compact(api);
    assert!(tenant.contains("ifletSome(id)=model.project_id{"));
    assert!(!tenant.contains("ifletSome(id)=Some(model.project_id){"));
    assert!(tenant.contains("project::Column::TenantId.eq(context.require_tenant()?)"));

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/operations-demo");
    let artifacts =
        appstruct_codegen::plan(&appstruct_compiler::compile_project(&fixture).unwrap()).unwrap();
    let compact = compact(artifact_text(&artifacts, "backend/src/api/order_line.rs"));
    assert_eq!(compact.matches("asyncfnvalidate_relations(").count(), 1);
    assert_eq!(
        compact
            .matches("validate_relations(state,context,&candidate).await?;")
            .count(),
        2
    );
}

#[test]
fn generated_relation_writes_enforce_target_read_validation() {
    let artifacts = appstruct_codegen::plan(&nullable_tenant_ir()).unwrap();
    let project = tempfile::tempdir().unwrap();
    for artifact in artifacts {
        let path = project.path().join(artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = project.path().join("backend");
    let manifest = backend.join("Cargo.toml");
    let mut manifest_source = fs::read_to_string(&manifest).unwrap();
    manifest_source.push_str(
        "\n[dev-dependencies]\ntower = { version = \"=0.5.3\", features = [\"util\"] }\n",
    );
    fs::write(&manifest, manifest_source).unwrap();
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(
        backend.join("tests/relation_access.rs"),
        include_str!("relation_access/runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&manifest, "relation_access");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn nullable_tenant_ir() -> AppIr {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let mut ir = appstruct_compiler::compile_project(&fixture).unwrap();
    let task = ir
        .entities
        .iter_mut()
        .find(|entity| entity.rust_name == "Task")
        .unwrap();
    let relation = task
        .fields
        .iter_mut()
        .find(|field| field.api_name == "project")
        .unwrap();
    relation.nullable = true;
    // Align the CSV column and DTO field so runtime requests reach relation validation.
    relation.api_name = relation.rust_name.clone();
    let field_id = relation.id.clone();
    ir.relations
        .iter_mut()
        .find(|relation| relation.foreign_key_fields.contains(&field_id))
        .unwrap()
        .required = false;
    ir
}

fn assert_write_calls(api: &str) {
    let compact = compact(api);
    assert_eq!(compact.matches("asyncfnvalidate_relations(").count(), 1);
    assert_eq!(
        compact
            .matches("validate_relations(&state,&context,&active).await?;")
            .count(),
        4,
        "create, update, bulk update, and CSV import must validate their post-hook candidates"
    );
}

fn compact(source: &str) -> String {
    source.split_whitespace().collect()
}

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}
