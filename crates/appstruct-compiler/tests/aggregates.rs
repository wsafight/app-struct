use appstruct_compiler::compile_project;
use appstruct_ir::validate_aggregates;
use std::path::Path;

fn demo() -> appstruct_ir::AppIr {
    compile_project(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/operations-demo"))
        .unwrap()
}

#[test]
fn resolves_aggregate_ownership_and_rejects_invalid_graphs() {
    let ir = demo();
    let order = ir
        .entities
        .iter()
        .position(|entity| entity.rust_name == "Order")
        .unwrap();
    let line = ir
        .entities
        .iter()
        .position(|entity| entity.rust_name == "OrderLine")
        .unwrap();
    let declaration = &ir.entities[order].views.aggregates[0];
    assert_eq!(declaration.child, ir.entities[line].id);
    assert_eq!(declaration.relation.0, "app::OrderLine.order");
    for invalid in 0..8 {
        let mut entities = ir.entities.clone();
        match invalid {
            0 => entities[order].views.aggregates[0].max_items = 101,
            1 => entities[order].views.aggregates[0].states.clear(),
            2 => entities[order].views.aggregates[0].states = vec!["missing".into()],
            3 => entities[line].tenant_scoped = false,
            4 => entities[line].views.soft_delete = true,
            5 => entities[order].views.aggregates.push(declaration.clone()),
            6 => entities[line].views.aggregates.push(declaration.clone()),
            _ => {
                entities[line]
                    .fields
                    .iter_mut()
                    .find(|field| field.id == declaration.relation)
                    .unwrap()
                    .nullable = true;
            }
        }
        assert!(
            validate_aggregates(&entities).is_err(),
            "invalid case {invalid}"
        );
    }
}
