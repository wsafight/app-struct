use appstruct_compiler::{ProjectLayout, compile_project, project_layout};
use appstruct_ir::FieldSemanticIr;
use std::path::{Path, PathBuf};

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/operations-demo")
}

#[test]
fn operations_demo_combines_existing_contracts() {
    assert_eq!(
        project_layout(&demo()).unwrap(),
        ProjectLayout::CompositionRoot
    );
    let ir = compile_project(&demo()).unwrap();
    assert_eq!(ir.entities.len(), 6);
    assert!(ir.tenant.enabled);
    assert!(ir.audit.enabled);
    assert!(ir.jobs.enabled);
    assert!(ir.file.enabled);
    assert!(ir.realtime.enabled);
    assert!(ir.report.enabled);
    assert!(ir.activity.enabled);
    assert_eq!(ir.activity.resources[0].resource, "orders");
    assert_eq!(ir.report.templates[0].name, "order-summary");

    let order = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Order")
        .unwrap();
    assert_eq!(order.workflow.as_ref().unwrap().transitions.len(), 3);
    let line = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "OrderLine")
        .unwrap();
    assert!(line.workflow.is_none());
    let price = line
        .fields
        .iter()
        .find(|field| field.api_name == "unit_price")
        .unwrap();
    assert!(matches!(
        price.ui_semantic,
        Some(FieldSemanticIr::Money {
            fraction_digits: 2,
            ..
        })
    ));
}
