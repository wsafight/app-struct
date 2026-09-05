use appstruct_ir::{
    AccessRuleIr, EntityId, FieldSemanticIr, FieldTypeIr, GeneratedValueIr, validate_app_ir,
};

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

#[test]
fn validates_money_ui_semantics_without_trusting_the_compiler() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    let entity = &mut ir.entities[0];
    let currency_id = entity
        .fields
        .iter_mut()
        .find(|field| field.api_name == "status")
        .map(|field| {
            field.ty = FieldTypeIr::Enum {
                values: vec!["CNY".to_owned(), "USD".to_owned()],
            };
            field.default = Some("CNY".to_owned());
            field.id.clone()
        })
        .unwrap();
    let amount = entity
        .fields
        .iter_mut()
        .find(|field| field.api_name == "name")
        .unwrap();
    amount.ty = FieldTypeIr::Decimal;
    amount.capabilities.searchable = false;
    amount.ui_semantic = Some(FieldSemanticIr::Money {
        currency_field: currency_id,
        fraction_digits: 2,
    });
    validate_app_ir(&ir).unwrap();

    let amount = ir.entities[0]
        .fields
        .iter_mut()
        .find(|field| field.api_name == "name")
        .unwrap();
    amount.ui_component = Some("MoneyEditor".to_owned());
    let errors = validate_app_ir(&ir).unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path.ends_with("ui_semantic"))
    );
}

#[test]
fn rejects_owner_rules_on_field_access() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    let field = ir.entities[0]
        .fields
        .iter_mut()
        .find(|field| field.rust_name == "name")
        .unwrap();
    field.read_access = Some(AccessRuleIr::Owner {
        field: field.id.clone(),
    });
    let errors = validate_app_ir(&ir).unwrap_err();
    assert!(errors.errors().iter().any(|error| {
        error.path.ends_with(".read_access")
            && error
                .message
                .contains("not supported for field-level access")
    }));
}

#[test]
fn rejects_ir_version_duplicates_and_incompatible_generated_values() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    ir.ir_version = 0;
    let duplicate = ir.entities[0].clone();
    ir.entities.push(duplicate);
    ir.entities[0].fields[0].generated = Some(GeneratedValueIr::Now);
    ir.entities[0].fields[0].default = Some("nope".to_owned());
    ir.entities[0].fields[0].validation.minimum = Some("nope".to_owned());
    let errors = validate_app_ir(&ir).unwrap_err();
    assert!(
        errors.to_string().contains("ir_version")
            || errors
                .errors()
                .iter()
                .any(|error| error.path == "ir_version")
    );
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path.contains("entities") && error.message.contains("duplicate"))
    );
}

#[test]
fn rejects_index_seed_and_relation_shape_errors() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    let entity_id = ir.entities[0].id.clone();
    let field_id = ir.entities[0].fields[0].id.clone();
    if let Some(index) = ir.entities[0].indexes.first_mut() {
        index.fields.clear();
        index.predicate = Some("email IS NOT NULL; DROP TABLE users".to_owned());
        index.entity = EntityId("app::Missing".to_owned());
    } else {
        ir.entities[0].indexes.push(appstruct_ir::IndexIr {
            id: "dup".to_owned(),
            entity: EntityId("app::Missing".to_owned()),
            fields: Vec::new(),
            unique: false,
            predicate: Some("x; y".to_owned()),
        });
        ir.entities[0].indexes.push(appstruct_ir::IndexIr {
            id: "dup".to_owned(),
            entity: entity_id,
            fields: vec![field_id.clone(), field_id],
            unique: false,
            predicate: None,
        });
    }
    ir.seeds.push(appstruct_ir::SeedIr {
        id: "missing".to_owned(),
        entity: EntityId("app::Missing".to_owned()),
        values: std::collections::BTreeMap::new(),
    });
    ir.seeds.push(appstruct_ir::SeedIr {
        id: "missing".to_owned(),
        entity: ir.entities[0].id.clone(),
        values: std::collections::BTreeMap::from([("nope".to_owned(), "1".to_owned())]),
    });
    ir.relations[0].foreign_key_fields.clear();
    ir.relations[0].foreign_key_owner = ir.relations[0].target.clone();
    ir.relations[0].inverse = Some(appstruct_ir::RelationId("missing".to_owned()));
    let nested = AccessRuleIr::Any {
        rules: vec![AccessRuleIr::Owner {
            field: ir.entities[0].fields[0].id.clone(),
        }],
    };
    ir.entities[0].fields[0].write_access = Some(nested.clone());
    ir.entities[0].access.update = AccessRuleIr::Owner {
        field: appstruct_ir::FieldId("missing".to_owned()),
    };
    ir.entities[0].access.delete = AccessRuleIr::All {
        rules: vec![AccessRuleIr::Public],
    };
    let errors = validate_app_ir(&ir).unwrap_err();
    assert!(errors.errors().len() >= 4);
}

#[test]
fn rejects_auth_graph_and_nested_field_owner_rules() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    ir.auth.user_entity = Some(EntityId("app::Missing".to_owned()));
    ir.auth.default_role = Some("unknown".to_owned());
    let errors = validate_app_ir(&ir).unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "auth.user_entity")
    );
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "auth.default_role")
    );

    let mut tenant = serde_json::from_str::<appstruct_ir::AppIr>(include_str!(
        "../../../tests/golden/m0-app-ir.json"
    ))
    .unwrap();
    tenant.auth.enabled = false;
    tenant.tenant.enabled = true;
    let errors = validate_app_ir(&tenant).unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "auth.enabled")
    );
}

#[test]
fn rejects_operation_and_module_graph_errors() {
    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    if let Some(command) = ir.commands.first_mut() {
        command.input = appstruct_ir::OperationTypeIr::Entity {
            entity: EntityId("app::Missing".to_owned()),
        };
        command.output = appstruct_ir::OperationTypeIr::ValueObject {
            value_object: "MissingInput".to_owned(),
        };
    }
    if let Some(query) = ir.queries.first_mut() {
        query.input = Some(appstruct_ir::OperationTypeIr::Entity {
            entity: EntityId("app::Missing".to_owned()),
        });
        query.output = appstruct_ir::OperationTypeIr::ValueObject {
            value_object: "MissingOutput".to_owned(),
        };
    }
    if let Some(module) = ir.modules.first_mut() {
        module.requires.push("missing.capability".to_owned());
    }
    if ir.modules.len() >= 2 {
        let name = ir.modules[0].name.clone();
        let order = ir.modules[0].startup_order;
        ir.modules[1].name = name;
        ir.modules[1].startup_order = order;
    }
    ir.auth.default_role = None;
    let errors = validate_app_ir(&ir).unwrap_err();
    assert!(errors.errors().len() >= 3);
}
