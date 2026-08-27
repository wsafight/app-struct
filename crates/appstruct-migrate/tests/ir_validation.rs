use appstruct_compiler::compile_project;
use appstruct_migrate::extract;
use std::path::Path;

#[test]
fn malformed_ir_is_rejected_before_schema_extraction() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.entities[0]
        .fields
        .iter_mut()
        .find(|field| field.primary_key)
        .unwrap()
        .primary_key = false;

    let error = extract(&ir).unwrap_err();
    assert!(
        error
            .errors()
            .iter()
            .any(|error| error.path == "entities[0].fields")
    );
}
