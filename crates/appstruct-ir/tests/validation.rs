use appstruct_ir::{EntityId, GeneratedValueIr, validate_app_ir};

#[test]
fn aggregates_semantic_invariant_violations_in_stable_path_order() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    ir.auth.user_entity = None;
    ir.entities[0]
        .fields
        .iter_mut()
        .find(|field| field.primary_key)
        .unwrap()
        .primary_key = false;
    ir.relations[0].target = EntityId("app::Missing".to_owned());

    let first = validate_app_ir(&ir).unwrap_err();
    let second = validate_app_ir(&ir).unwrap_err();
    assert_eq!(first, second);
    assert!(first.errors().len() >= 3);
    assert!(
        first
            .errors()
            .windows(2)
            .all(|pair| pair[0].path <= pair[1].path)
    );
    assert!(
        first
            .errors()
            .iter()
            .any(|error| error.path == "auth.user_entity")
    );
    assert!(first.errors().iter().any(|error| {
        error.path == "relations[0].target" && error.message.contains("app::Missing")
    }));
}

#[test]
fn accepts_the_internal_revision_default() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    let entity = &mut ir.entities[0];
    let revision = entity
        .fields
        .iter_mut()
        .find(|field| field.api_name == "revision")
        .unwrap();
    revision.generated = Some(GeneratedValueIr::Revision);
    revision.default = Some("1".to_owned());

    validate_app_ir(&ir).unwrap();
}
