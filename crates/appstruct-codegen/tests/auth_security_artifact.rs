use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::path::Path;

#[test]
fn generated_auth_security_guards_are_emitted() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.auth.oauth_enabled = true;
    ir.auth.oauth_providers = vec!["oidc".to_owned()];
    let artifacts = plan(&ir).unwrap();

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("PRIMARY KEY (\"provider\", \"subject\")"));
    assert!(!sql.contains("\"subject\" TEXT NOT NULL PRIMARY KEY"));

    let oauth = artifact_text(&artifacts, "backend/src/auth/oauth.rs");
    assert!(oauth.contains("a.email_verified_at"));
    assert!(oauth.contains("FOR UPDATE OF a"));
    assert!(oauth.contains("The local account email must be verified before OAuth login"));
    let compact = oauth.split_whitespace().collect::<String>();
    assert!(compact.contains("row.try_get(\"user_id\")?"));
    let account_lock = oauth
        .find(r#"SELECT password_hash FROM \"_appstruct_auth_accounts\" WHERE user_id = $1 FOR UPDATE"#)
        .expect("unlink locks the account before counting login methods");
    let oauth_count = oauth
        .find(r#"SELECT COUNT(*) AS oauth_count FROM \"_appstruct_auth_oauth_accounts\" WHERE user_id = $1"#)
        .expect("unlink counts providers after acquiring the account lock");
    assert!(account_lock < oauth_count);

    let tenant = artifact_text(&artifacts, "backend/src/tenant.rs");
    let compact = tenant.split_whitespace().collect::<String>();
    assert!(compact.contains("ifcrate::auth::account_email_verified(&state,user).await?"));
    assert!(compact.contains("if!crate::auth::account_email_verified(&state,actor.id).await?"));
}

fn artifact_text<'a>(artifacts: &'a [Artifact], path: &str) -> &'a str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap_or_else(|| panic!("missing generated artifact {path}"));
    std::str::from_utf8(&artifact.content).unwrap()
}
