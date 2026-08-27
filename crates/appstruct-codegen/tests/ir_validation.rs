use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use appstruct_ir::EntityId;
use std::path::Path;

#[test]
fn malformed_ir_is_rejected_before_generators_traverse_it() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.relations[0].target = EntityId("app::Missing".to_owned());

    let error = plan(&ir).unwrap_err();
    assert!(error.to_string().contains("relations[0].target"));
}
