use appstruct_compiler::compile_project;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

#[test]
fn rejects_access_roles_that_rbac_does_not_declare() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/identity.yaml"),
        "        role: member",
        "        role: auditor",
    );

    assert_diagnostic(temporary.path(), "AS3030");
}

#[test]
fn rejects_owner_fields_that_do_not_relate_to_the_auth_user() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/project.yaml"),
        "        target: User",
        "        target: Project",
    );

    assert_diagnostic(temporary.path(), "AS3033");
}

#[test]
fn rejects_empty_composite_access_rules() {
    for operator in ["any", "all"] {
        let temporary = copied_fixture();
        replace(
            &temporary.path().join("spec/project.yaml"),
            concat!(
                "      list:\n",
                "        any:\n",
                "          - owner: owner\n",
                "          - role: admin",
            ),
            &format!("      list:\n        {operator}: []"),
        );

        assert_diagnostic(temporary.path(), "AS1007");
    }
}

#[test]
fn rejects_incompatible_auth_user_identity_fields() {
    for (old, new) in [
        ("        type: uuid", "        type: integer"),
        ("        unique: true", "        unique: false"),
    ] {
        let temporary = copied_fixture();
        replace(&temporary.path().join("spec/identity.yaml"), old, new);

        assert_diagnostic(temporary.path(), "AS3028");
    }
}

#[test]
fn lowers_field_read_and_write_access_rules() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("spec/project.yaml"),
        "        max_length: 120\n",
        concat!(
            "        max_length: 120\n",
            "        access:\n",
            "          read: { role: admin }\n",
            "          write: { role: admin }\n",
        ),
    );
    let ir = compile_project(temporary.path()).unwrap();
    let field = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Project")
        .unwrap()
        .fields
        .iter()
        .find(|field| field.rust_name == "name")
        .unwrap();
    assert!(matches!(
        field.read_access,
        Some(appstruct_ir::AccessRuleIr::Role { ref role }) if role == "admin"
    ));
    assert!(matches!(
        field.write_access,
        Some(appstruct_ir::AccessRuleIr::Role { ref role }) if role == "admin"
    ));
}

#[test]
fn lowers_configured_social_providers_and_rejects_unknown_provider() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    password_reset: true\n",
        "    password_reset: true\n    providers: [google, github]\n",
    );
    let ir = compile_project(temporary.path()).unwrap();
    assert_eq!(
        ir.auth.oauth_providers,
        vec!["github".to_owned(), "google".to_owned()]
    );
    assert!(ir.auth.oauth_enabled);

    let invalid = copied_fixture();
    replace(
        &invalid.path().join("appstruct.yaml"),
        "    password_reset: true\n",
        "    password_reset: true\n    providers: [linkedin]\n",
    );
    assert_diagnostic(invalid.path(), "AS3027");
}

#[test]
fn lowers_configured_stripe_billing_and_rejects_unsupported_capability() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    default_role: member\n",
        concat!(
            "    default_role: member\n",
            "  tenant:\n",
            "    enabled: true\n",
            "  billing:\n",
            "    enabled: true\n",
            "    provider: stripe\n",
            "    capabilities: { subscriptions: true, trials: true }\n",
            "    plans:\n",
            "      - id: pro\n",
            "        price_env: APPSTRUCT_STRIPE_PRICE_PRO\n",
            "        trial_days: 14\n",
            "        entitlements: [projects]\n",
        ),
    );
    let ir = compile_project(temporary.path()).unwrap();
    assert!(ir.billing.enabled);
    assert_eq!(ir.billing.provider.as_deref(), Some("stripe"));
    assert_eq!(ir.billing.plans[0].trial_days, Some(14));

    let invalid = copied_fixture();
    replace(
        &invalid.path().join("appstruct.yaml"),
        "    default_role: member\n",
        concat!(
            "    default_role: member\n",
            "  tenant:\n",
            "    enabled: true\n",
            "  billing:\n",
            "    enabled: true\n",
            "    provider: stripe\n",
            "    capabilities: { subscriptions: true, metered_usage: true }\n",
            "    plans: [{ id: pro, price_env: APPSTRUCT_STRIPE_PRICE_PRO }]\n",
        ),
    );
    assert_diagnostic(invalid.path(), "AS3075");
}

fn copied_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(fixture().join(relative), temporary.path().join(relative)).unwrap();
    }
    temporary
}

fn replace(path: &Path, old: &str, new: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains(old));
    fs::write(path, source.replacen(old, new, 1)).unwrap();
}

fn assert_diagnostic(project: &Path, code: &str) {
    let diagnostics = compile_project(project).unwrap_err();
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "expected {code}, got {diagnostics:#?}"
    );
}
