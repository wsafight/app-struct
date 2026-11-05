use appstruct_compiler::compile_project;
use appstruct_ir::PageScopeIr;
use std::{fs, path::Path};

fn project(pages: &str) -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(root.join(relative), temporary.path().join(relative)).unwrap();
    }
    let domain = temporary.path().join("spec/project.yaml");
    let source = fs::read_to_string(&domain).unwrap();
    fs::write(domain, format!("{source}\npages:\n{pages}")).unwrap();
    temporary
}

#[test]
fn separates_public_account_and_existing_workspace_pages() {
    let temporary = project(
        "  Landing:\n    path: /\n    component: Landing\n    scope: public\n  Profile:\n    path: account\n    component: Profile\n    scope: account\n  Studio:\n    path: studio\n    component: Studio\n",
    );
    let ir = compile_project(temporary.path()).unwrap();
    assert_eq!(
        ir.pages.iter().map(|page| page.scope).collect::<Vec<_>>(),
        [
            PageScopeIr::Public,
            PageScopeIr::Account,
            PageScopeIr::Workspace
        ]
    );
}

#[test]
fn rejects_private_root_and_generated_routes_in_all_scopes() {
    for (path, scope) in [
        ("/", "workspace"),
        ("admin/users", "public"),
        ("projects/edit", "account"),
        ("account", "public"),
        ("workspace", "workspace"),
        ("login", "public"),
    ] {
        let temporary = project(&format!(
            "  Page:\n    path: {path}\n    component: Page\n    scope: {scope}\n"
        ));
        assert!(
            compile_project(temporary.path()).is_err(),
            "{path} in {scope}"
        );
    }
}

#[test]
fn rejects_unknown_scope_and_account_without_auth() {
    let temporary = project("  Page:\n    path: studio\n    component: Page\n    scope: private\n");
    assert!(compile_project(temporary.path()).is_err());
    let temporary = tempfile::tempdir().unwrap();
    fs::write(temporary.path().join("appstruct.yaml"), "version: 1\napp:\n  name: public-pages\ndatabase:\n  provider: postgres\nincludes: [pages.yaml]\n").unwrap();
    fs::write(temporary.path().join("pages.yaml"), "domain: pages\npages:\n  Account:\n    path: profile\n    scope: account\n    component: Account\n").unwrap();
    let errors = compile_project(temporary.path()).unwrap_err();
    assert!(errors.iter().any(|error| error.code == "AS3034"));
}

#[test]
fn rejects_resource_collisions_with_account_and_file_library() {
    for table in ["account", "files", "onboarding", "workspace"] {
        let temporary =
            project("  Landing:\n    path: /\n    scope: public\n    component: Landing\n");
        let domain = temporary.path().join("spec/project.yaml");
        let source = fs::read_to_string(&domain)
            .unwrap()
            .replace("table: projects", &format!("table: {table}"));
        fs::write(domain, source).unwrap();
        let root = temporary.path().join("appstruct.yaml");
        let source = fs::read_to_string(&root)
            .unwrap()
            .replace("includes:", "  file:\n    enabled: true\n\nincludes:");
        fs::write(root, source).unwrap();
        let errors = compile_project(temporary.path()).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "AS3012"),
            "{table}: {errors:?}"
        );
    }
}
