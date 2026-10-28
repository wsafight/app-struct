use std::{fs, path::PathBuf};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/form-rules-project")
}

#[test]
fn typed_sibling_rules_lower_and_reject_invalid_or_hidden_dependencies() {
    let ir = appstruct_compiler::compile_project(&fixture()).unwrap();
    appstruct_ir::validate_app_ir(&ir).unwrap();
    let entity = &ir.entities[0];
    let amount = entity
        .fields
        .iter()
        .find(|field| field.api_name == "amount")
        .unwrap();
    assert_eq!(
        amount.validation.greater_than.as_ref().unwrap().0,
        "app::Appointment.minimum"
    );
    for (from, to) in [
        ("greater_than: starts", "greater_than: unknown"),
        ("greater_than: starts", "greater_than: ends"),
        ("greater_than: starts", "greater_than: minimum"),
        (
            "field: needs_note, equals: false",
            "field: needs_note, equals: \"false\"",
        ),
        (
            "field: status, equals: ready",
            "field: status, equals: missing",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("appstruct.yaml"),
            fs::read(fixture().join("appstruct.yaml")).unwrap(),
        )
        .unwrap();
        fs::create_dir(root.path().join("spec")).unwrap();
        let source = fs::read_to_string(fixture().join("spec/appointment.yaml"))
            .unwrap()
            .replace(from, to);
        fs::write(root.path().join("spec/appointment.yaml"), source).unwrap();
        let errors = appstruct_compiler::compile_project(root.path()).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "AS2045"),
            "{errors:?}"
        );
    }
    // Imported IR is checked independently of the YAML compiler.
    let mut hidden = ir.clone();
    hidden.entities[0]
        .fields
        .iter_mut()
        .find(|field| field.api_name == "starts")
        .unwrap()
        .read_access = Some(appstruct_ir::AccessRuleIr::Authenticated);
    assert!(appstruct_ir::validate_form_rules(&hidden.entities).is_err());
    let mut invalid = ir;
    invalid.entities[0]
        .fields
        .iter_mut()
        .find(|field| field.api_name == "amount")
        .unwrap()
        .validation
        .greater_than = Some(appstruct_ir::FieldId("outside::value".into()));
    assert!(appstruct_ir::validate_app_ir(&invalid).is_err());
}
