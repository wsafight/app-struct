use appstruct_compiler::compile_project;
use appstruct_ir::AccessRuleIr;
use std::{fs, path::Path};

#[test]
fn lowers_entitlement_access_for_business_operations() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), temporary.path().join(relative)).unwrap();
    }
    let appstruct = temporary.path().join("appstruct.yaml");
    let mut app = fs::read_to_string(&appstruct).unwrap();
    app = app.replacen(
        "  tenant:\n    enabled: true\n",
        "  tenant:\n    enabled: true\n  billing:\n    enabled: true\n    provider: stripe\n    capabilities:\n      subscriptions: true\n    plans:\n      - id: pro\n        price_env: APPSTRUCT_STRIPE_PRICE_PRO\n        entitlements: [reports]\n",
        1,
    );
    fs::write(appstruct, app).unwrap();
    let project = temporary.path().join("spec/project.yaml");
    let mut spec = fs::read_to_string(&project).unwrap();
    spec.push_str(
        "\ncommands:\n  GenerateReport:\n    input: Project\n    output: Project\n    access:\n      all:\n        - authenticated: true\n        - entitlement: reports\n",
    );
    fs::write(project, spec).unwrap();

    let ir = compile_project(temporary.path()).unwrap();
    assert_eq!(
        ir.commands[0].access,
        AccessRuleIr::All {
            rules: vec![
                AccessRuleIr::Authenticated,
                AccessRuleIr::Entitlement {
                    entitlement: "reports".to_owned()
                },
            ],
        }
    );
}

#[test]
fn rejects_entitlement_access_without_a_declared_plan_entitlement() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), temporary.path().join(relative)).unwrap();
    }
    let project = temporary.path().join("spec/project.yaml");
    let mut spec = fs::read_to_string(&project).unwrap();
    spec.push_str("\ncommands:\n  GenerateReport:\n    input: Project\n    output: Project\n    access: { entitlement: reports }\n");
    fs::write(project, spec).unwrap();
    let errors = compile_project(temporary.path()).unwrap_err();
    assert!(errors.iter().any(|error| error.code == "AS3079"));
}

#[test]
fn report_allowance_requires_a_declared_entitlement_and_bounded_limit() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo");
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in [
        "appstruct.yaml",
        "appstruct.lock",
        "spec/identity.yaml",
        "spec/work.yaml",
        "spec/product.yaml",
        "spec/collaboration.yaml",
    ] {
        fs::copy(source.join(relative), temporary.path().join(relative)).unwrap();
    }
    let ir = compile_project(temporary.path()).unwrap();
    assert_eq!(ir.report.entitlement.as_deref(), Some("reports"));
    assert_eq!(ir.report.free_runs, Some(3));
    let root = temporary.path().join("appstruct.yaml");
    let original = fs::read_to_string(&root).unwrap();
    for replacement in [
        original.replace("    entitlement: reports\n", ""),
        original.replace("    entitlement: reports", "    entitlement: missing"),
        original.replace("    free_runs: 3", "    free_runs: 4294967296"),
        original.replace(
            "  billing:\n    enabled: true",
            "  billing:\n    enabled: false",
        ),
    ] {
        fs::write(&root, replacement).unwrap();
        let errors = compile_project(temporary.path()).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "AS3098"),
            "{errors:?}"
        );
    }
}
