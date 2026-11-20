#[allow(dead_code)]
mod support;

use std::{fs, path::Path};

#[test]
fn generated_file_and_audit_lifecycle() {
    assert_lifecycle(
        "examples/operations-demo",
        include_str!("data_lifecycle/runtime.rs"),
    );
}

#[test]
fn disabled_file_reconciliation_keeps_its_public_contract() {
    assert_lifecycle(
        "tests/fixtures/m2-project",
        include_str!("data_lifecycle/disabled.rs"),
    );
}

#[test]
fn generated_file_semantic_aggregate_create_and_bounded_count_lifecycle() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/operations-demo");
    let mut ir = appstruct_compiler::compile_project(&fixture).unwrap();
    push_file_field(&mut ir, "Product", "datasheet");
    push_file_field(&mut ir, "Product", "manual");
    push_file_field(&mut ir, "Order", "attachment");
    push_file_field(&mut ir, "OrderLine", "spec_file");
    let project = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = project.path().join(&artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = project.path().join("backend");
    let manifest = backend.join("Cargo.toml");
    let mut source = fs::read_to_string(&manifest).unwrap();
    source.push_str(
        "\n[dev-dependencies]\ntower = { version = \"=0.5.3\", features = [\"util\"] }\n",
    );
    fs::write(&manifest, source).unwrap();
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(
        backend.join("tests/semantic_lifecycle.rs"),
        include_str!("data_lifecycle/semantic_runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&manifest, "semantic_lifecycle");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn push_file_field(ir: &mut appstruct_ir::AppIr, entity_name: &str, field_name: &str) {
    let entity = ir
        .entities
        .iter_mut()
        .find(|candidate| candidate.rust_name == entity_name)
        .unwrap();
    let entity_id = entity.id.clone();
    entity.fields.push(appstruct_ir::FieldIr {
        id: appstruct_ir::FieldId(format!("{entity_id}.{field_name}")),
        entity: entity_id,
        rust_name: field_name.to_owned(),
        api_name: field_name.to_owned(),
        column_name: field_name.to_owned(),
        ty: appstruct_ir::FieldTypeIr::Uuid,
        nullable: true,
        primary_key: false,
        unique: false,
        generated: None,
        default: None,
        validation: appstruct_ir::ValidationIr::default(),
        capabilities: appstruct_ir::FieldCapabilities {
            filterable: false,
            sortable: false,
            searchable: false,
        },
        read_access: None,
        write_access: None,
        ui_component: None,
        ui_semantic: Some(appstruct_ir::FieldSemanticIr::File),
    });
}

fn assert_lifecycle(fixture: &str, runtime: &str) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(fixture);
    let ir = appstruct_compiler::compile_project(&fixture).unwrap();
    let project = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = project.path().join(&artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = project.path().join("backend");
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(backend.join("tests/lifecycle.rs"), runtime).unwrap();
    let output = support::cargo_test(&backend.join("Cargo.toml"), "lifecycle");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
