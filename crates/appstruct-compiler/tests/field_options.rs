use appstruct_compiler::compile_project;
use appstruct_ir::FieldSemanticIr;
use std::fs;

fn compile_fields(fields: &str) -> Result<appstruct_ir::AppIr, Vec<appstruct_ir::Diagnostic>> {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: field-options\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        format!(
            "domain: core\nentities:\n  Item:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n{fields}    access:\n      list: {{ public: true }}\n      read: {{ public: true }}\n      create: {{ public: true }}\n      update: {{ public: true }}\n      delete: {{ public: true }}\n"
        ),
    )
    .unwrap();
    compile_project(project.path())
}

fn codes(diagnostics: &[appstruct_ir::Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

#[test]
fn resolves_display_fields_and_rejects_missing_or_non_label_values() {
    let ir = compile_fields("      name: {type: string}\n    display_field: name\n").unwrap();
    assert_eq!(
        ir.entities[0].views.display_field.as_ref().unwrap().0,
        "app::Item.name"
    );
    for field in ["missing", "amount"] {
        let diagnostics = compile_fields(&format!(
            "      amount: {{type: decimal}}\n    display_field: {field}\n"
        ))
        .unwrap_err();
        assert!(codes(&diagnostics).contains(&"AS2042"));
    }
}

#[test]
fn rejects_length_bounds_on_non_string_fields() {
    let diagnostics = compile_fields(
        "      count:\n        type: integer\n        min_length: 1\n        max_length: 2\n",
    )
    .unwrap_err();
    assert!(codes(&diagnostics).contains(&"AS2012"));
}

#[test]
fn rejects_inverted_string_length_bounds() {
    let diagnostics = compile_fields(
        "      name:\n        type: string\n        min_length: 8\n        max_length: 2\n",
    )
    .unwrap_err();
    assert!(codes(&diagnostics).contains(&"AS2013"));
}

#[test]
fn rejects_numeric_bounds_on_text_and_inverted_integer_bounds() {
    let non_numeric = compile_fields(
        "      name:\n        type: string\n        minimum: 1\n        maximum: 2\n",
    )
    .unwrap_err();
    assert!(codes(&non_numeric).contains(&"AS2012"));
    let inverted = compile_fields(
        "      count:\n        type: integer\n        minimum: 9\n        maximum: 1\n",
    )
    .unwrap_err();
    assert!(codes(&inverted).contains(&"AS2018"));
}

#[test]
fn rejects_invalid_numeric_bound_literals() {
    let diagnostics =
        compile_fields("      count:\n        type: integer\n        minimum: nope\n").unwrap_err();
    assert!(codes(&diagnostics).contains(&"AS2017"));
}

#[test]
fn rejects_values_target_searchable_and_json_filter_options() {
    let values =
        compile_fields("      name:\n        type: string\n        values: [a]\n").unwrap_err();
    assert!(codes(&values).contains(&"AS2012"));
    let relation_options = compile_fields(
        "      name:\n        type: string\n        target: Item\n        on_delete: cascade\n",
    )
    .unwrap_err();
    assert!(codes(&relation_options).contains(&"AS2012"));
    let searchable =
        compile_fields("      count:\n        type: integer\n        searchable: true\n")
            .unwrap_err();
    assert!(codes(&searchable).contains(&"AS2012"));
    let json = compile_fields(
        "      payload:\n        type: json\n        filterable: true\n        sortable: true\n",
    )
    .unwrap_err();
    assert!(codes(&json).contains(&"AS2012"));
}

#[test]
fn rejects_invalid_defaults_and_generated_combinations() {
    let invalid_default =
        compile_fields("      count:\n        type: integer\n        default: nope\n").unwrap_err();
    assert!(codes(&invalid_default).contains(&"AS2014"));
    let both = compile_fields(
        "      created_at:\n        type: datetime\n        generated: now\n        default: 2020-01-01T00:00:00Z\n",
    )
    .unwrap_err();
    assert!(codes(&both).contains(&"AS2019"));
    let incompatible =
        compile_fields("      name:\n        type: string\n        generated: uuid_v7\n")
            .unwrap_err();
    assert!(codes(&incompatible).contains(&"AS2015"));
}

#[test]
fn rejects_invalid_ui_components_and_non_id_primary_keys() {
    let component = compile_fields(
        "      name:\n        type: string\n        ui:\n          component: not-a-type\n",
    )
    .unwrap_err();
    assert!(codes(&component).contains(&"AS3011"));
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: field-options\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  Item:\n    fields:\n      name:\n        type: string\n        primary_key: true\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();
    let diagnostics = compile_project(project.path()).unwrap_err();
    assert!(codes(&diagnostics).contains(&"AS2012"));
}

#[test]
fn accepts_compatible_generated_values() {
    compile_fields(
        "      created_at:\n        type: datetime\n        generated: now\n      count:\n        type: integer\n        generated: auto_increment\n",
    )
    .unwrap();
}

#[test]
fn accepts_money_ui_semantics_and_resolves_the_currency_field() {
    let ir = compile_fields(
        "      amount:\n        type: decimal\n        required: true\n        ui:\n          semantic: money\n          currency_field: currency\n          fraction_digits: 2\n      currency:\n        type: enum\n        required: true\n        values: [CNY, USD]\n",
    )
    .unwrap();
    let amount = ir.entities[0]
        .fields
        .iter()
        .find(|field| field.api_name == "amount")
        .unwrap();
    assert!(matches!(
        amount.ui_semantic,
        Some(FieldSemanticIr::Money {
            ref currency_field,
            fraction_digits: 2,
        }) if currency_field.0 == "app::Item.currency"
    ));
}

#[test]
fn rejects_invalid_money_ui_semantics() {
    let cases = [
        "      amount:\n        type: string\n        ui:\n          semantic: money\n          currency_field: currency\n      currency:\n        type: enum\n        values: [CNY]\n",
        "      amount:\n        type: decimal\n        ui:\n          semantic: money\n          currency_field: missing\n",
        "      amount:\n        type: decimal\n        ui:\n          semantic: money\n          currency_field: currency\n          fraction_digits: 7\n      currency:\n        type: enum\n        values: [CNY]\n",
        "      amount:\n        type: decimal\n        required: true\n        ui:\n          semantic: money\n          currency_field: currency\n      currency:\n        type: enum\n        values: [cny]\n",
        "      amount:\n        type: decimal\n        required: true\n        ui:\n          semantic: money\n          currency_field: currency\n      currency:\n        type: enum\n        values: [CNY]\n",
        "      amount:\n        type: decimal\n        ui:\n          semantic: money\n          currency_field: currency\n      currency:\n        type: enum\n        values: [CNY]\n        ui:\n          component: CurrencyPicker\n",
        "      subtotal:\n        type: decimal\n        ui:\n          semantic: money\n          currency_field: currency\n      total:\n        type: decimal\n        ui:\n          semantic: money\n          currency_field: currency\n      currency:\n        type: enum\n        values: [CNY]\n",
    ];
    for fields in cases {
        let diagnostics = compile_fields(fields).unwrap_err();
        assert!(
            codes(&diagnostics).contains(&"AS2020"),
            "missing money diagnostic for {fields}"
        );
    }
}

#[test]
fn rejects_invalid_field_and_column_names_and_duplicate_columns() {
    let name = compile_fields("      BadName:\n        type: string\n").unwrap_err();
    assert!(codes(&name).contains(&"AS2001"));
    let column = compile_fields("      title:\n        type: string\n        column: Bad Column\n")
        .unwrap_err();
    assert!(codes(&column).contains(&"AS2001"));
    let duplicate = compile_fields(
        "      title:\n        type: string\n        column: shared\n      note:\n        type: string\n        column: shared\n",
    )
    .unwrap_err();
    assert!(codes(&duplicate).contains(&"AS2005"));
}

#[test]
fn rejects_required_relations_with_set_null() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: relations\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  User:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n  Item:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n      owner:\n        type: relation\n        target: User\n        required: true\n        on_delete: set_null\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();
    let diagnostics = compile_project(project.path()).unwrap_err();
    assert!(codes(&diagnostics).contains(&"AS2006"));
}

#[test]
fn rejects_invalid_and_conflicting_value_objects() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: values\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        "domain: core\nentities:\n  Item:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\nvalue_objects:\n  Item:\n    fields:\n      reason:\n        type: string\n  bad-name:\n    fields:\n      reason:\n        type: string\n  Empty:\n    fields: {}\n",
    )
    .unwrap();
    let diagnostics = compile_project(project.path()).unwrap_err();
    let found = codes(&diagnostics);
    assert!(found.contains(&"AS3003") || found.contains(&"AS3004") || found.contains(&"AS3005"));
}
