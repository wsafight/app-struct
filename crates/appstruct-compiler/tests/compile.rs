use appstruct_compiler::{compile_project, compile_project_report};
use appstruct_ir::{ModuleOrigin, OperationTypeIr, to_canonical_json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

#[test]
fn compiles_fixture_to_canonical_golden() {
    let ir = compile_project(&fixture()).unwrap();
    let actual = to_canonical_json(&ir).unwrap();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/m0-app-ir.json"),
            &actual,
        )
        .unwrap();
    }
    let expected = include_str!("../../../tests/golden/m0-app-ir.json");
    assert_eq!(actual, expected);
}

#[test]
fn reports_public_mutations_without_blocking_compilation() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let report = compile_project_report(&fixture).unwrap();
    assert_eq!(report.ir.entities.len(), 2);
    assert_eq!(report.diagnostics.len(), 2);
    assert!(report.diagnostics.iter().all(|diagnostic| {
        diagnostic.code == "AS3070" && diagnostic.severity == appstruct_ir::Severity::Warning
    }));
    assert!(compile_project(&fixture).is_ok());
}

#[test]
fn compiles_m3_extension_contracts() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m3-project");
    let ir = compile_project(&fixture).unwrap();

    assert_eq!(ir.value_objects.len(), 2);
    assert_eq!(ir.commands[0].rust_name, "ArchiveProject");
    assert!(matches!(
        ir.commands[0].output,
        OperationTypeIr::Entity { .. }
    ));
    assert_eq!(ir.queries[0].rust_name, "ProjectMetrics");
    assert!(ir.queries[0].input.is_none());
    assert_eq!(ir.pages[0].component, "ProjectDashboard");
    assert_eq!(
        ir.entities[0]
            .fields
            .iter()
            .find(|field| field.rust_name == "metadata")
            .unwrap()
            .ui_component
            .as_deref(),
        Some("ProjectMetadataEditor")
    );
}

#[test]
fn resolves_official_module_capabilities_into_ir() {
    let ir = compile_project(&fixture()).unwrap();
    assert_eq!(
        ir.modules
            .iter()
            .map(|module| module.name.as_str())
            .collect::<Vec<_>>(),
        ["appstruct/auth", "appstruct/rbac"]
    );
    assert_eq!(ir.modules[0].provides, ["auth.identity"]);
    assert_eq!(ir.modules[1].requires, ["auth.identity"]);
    assert_eq!(ir.modules[1].startup_order, 1);
}

#[test]
fn loads_local_modules_and_artifacts_into_the_capability_graph() {
    let temporary = tempfile::tempdir().unwrap();
    copy_fixture(&fixture(), temporary.path());
    add_module_declaration(temporary.path(), "modules/example/module.toml");
    fs::create_dir_all(temporary.path().join("modules/example/assets")).unwrap();
    let manifest = concat!(
        "api_version = 1\n",
        "name = \"local/example\"\n",
        "version = \"0.1.0\"\n",
        "provides = [\"example.docs\"]\n",
        "requires = [\"auth.identity\"]\n\n",
        "[[artifacts]]\n",
        "path = \"docs/README.md\"\n",
        "source = \"assets/README.md\"\n",
    );
    fs::write(
        temporary.path().join("modules/example/module.toml"),
        manifest,
    )
    .unwrap();
    fs::write(
        temporary.path().join("modules/example/assets/README.md"),
        "# Local module\n",
    )
    .unwrap();

    let ir = compile_project(temporary.path()).unwrap();
    let module = ir
        .modules
        .iter()
        .find(|module| module.name == "local/example")
        .unwrap();
    assert_eq!(module.origin, ModuleOrigin::Local);
    assert_eq!(
        module.manifest_path.as_deref(),
        Some("modules/example/module.toml")
    );
    assert_eq!(
        module.content_sha256.as_deref(),
        Some(format!("sha256:{:x}", Sha256::digest(manifest.as_bytes())).as_str())
    );
    assert_eq!(module.requires, ["auth.identity"]);
    assert_eq!(module.artifacts[0].path, "docs/README.md");
    assert_eq!(
        module.artifacts[0].source.as_deref(),
        Some("modules/example/assets/README.md")
    );
    assert_eq!(module.artifacts[0].byte_len, 15);
    assert_eq!(
        module.artifacts[0].sha256,
        format!("sha256:{:x}", Sha256::digest(b"# Local module\n"))
    );
    assert_eq!(module.artifacts[0].content, "# Local module\n");
    assert!(module.startup_order > ir.modules[0].startup_order);
}

#[test]
fn rejects_unsafe_or_unsupported_local_module_manifests() {
    let traversal = tempfile::tempdir().unwrap();
    copy_fixture(&fixture(), traversal.path());
    add_module_declaration(traversal.path(), "../module.toml");
    let diagnostics = compile_project(traversal.path()).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS1013");

    let future = tempfile::tempdir().unwrap();
    copy_fixture(&fixture(), future.path());
    add_module_declaration(future.path(), "modules/example/module.toml");
    fs::create_dir_all(future.path().join("modules/example")).unwrap();
    fs::write(
        future.path().join("modules/example/module.toml"),
        "api_version = 2\nname = \"local/example\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    let diagnostics = compile_project(future.path()).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS3063");
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_local_module_artifacts() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    copy_fixture(&fixture(), temporary.path());
    add_module_declaration(temporary.path(), "modules/example/module.toml");
    fs::create_dir_all(temporary.path().join("modules/example/assets")).unwrap();
    fs::write(
        temporary.path().join("modules/example/module.toml"),
        concat!(
            "api_version = 1\n",
            "name = \"local/example\"\n",
            "version = \"1.0.0\"\n\n",
            "[[artifacts]]\n",
            "path = \"README.md\"\n",
            "source = \"assets/README.md\"\n",
        ),
    )
    .unwrap();
    fs::write(temporary.path().join("outside.md"), "outside\n").unwrap();
    symlink(
        temporary.path().join("outside.md"),
        temporary.path().join("modules/example/assets/README.md"),
    )
    .unwrap();

    let diagnostics = compile_project(temporary.path()).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS1013");
    assert!(diagnostics[0].message.contains("symbolic link"));
}

#[test]
fn rejects_custom_page_paths_that_shadow_generated_routes() {
    let temporary = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m3-project");
    fs::create_dir(temporary.path().join("spec")).unwrap();
    fs::copy(
        fixture.join("appstruct.yaml"),
        temporary.path().join("appstruct.yaml"),
    )
    .unwrap();
    fs::copy(
        fixture.join("spec/project.yaml"),
        temporary.path().join("spec/project.yaml"),
    )
    .unwrap();
    let spec_path = temporary.path().join("spec/project.yaml");
    let spec = fs::read_to_string(&spec_path).unwrap();
    fs::write(
        &spec_path,
        spec.replace("path: project-dashboard", "path: projects/new"),
    )
    .unwrap();

    let diagnostics = compile_project(temporary.path()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS3012")
    );
}

#[test]
fn include_order_does_not_change_ir() {
    let expected = compile_project(&fixture()).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    copy_fixture(&fixture(), temporary.path());
    let root_file = temporary.path().join("appstruct.yaml");
    let root = fs::read_to_string(&root_file).unwrap();
    let reordered = root.replace(
        "  - spec/project.yaml\n  - spec/identity.yaml",
        "  - spec/identity.yaml\n  - spec/project.yaml",
    );
    fs::write(root_file, reordered).unwrap();

    let actual = compile_project(temporary.path()).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn accumulates_semantic_diagnostics() {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    fs::write(
        temporary.path().join("appstruct.yaml"),
        "version: 1\napp: { name: demo }\ndatabase: { provider: postgres }\nincludes: [spec/domain.yaml]\n",
    )
    .unwrap();
    fs::write(
        temporary.path().join("spec/domain.yaml"),
        "domain: demo\nentities:\n  Broken:\n    fields:\n      value:\n        type: mystery\n",
    )
    .unwrap();

    let diagnostics = compile_project(temporary.path()).unwrap_err();
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"AS2004"));
    assert!(codes.contains(&"AS2007"));
    assert!(codes.contains(&"AS3001"));
}

#[test]
fn rejects_include_that_escapes_project_root() {
    let outer = tempfile::tempdir().unwrap();
    let project = outer.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join("appstruct.yaml"),
        "version: 1\napp: { name: demo }\ndatabase: { provider: postgres }\nincludes: [../outside.yaml]\n",
    )
    .unwrap();
    fs::write(
        outer.path().join("outside.yaml"),
        "domain: outside\nentities: {}\n",
    )
    .unwrap();

    let diagnostics = compile_project(&project).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS1010");
}

#[test]
fn rejects_unknown_field_keys_instead_of_ignoring_typos() {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    fs::write(
        temporary.path().join("appstruct.yaml"),
        "version: 1\napp: { name: demo }\ndatabase: { provider: postgres }\nincludes: [spec/domain.yaml]\n",
    )
    .unwrap();
    fs::write(
        temporary.path().join("spec/domain.yaml"),
        concat!(
            "domain: demo\n",
            "entities:\n",
            "  Project:\n",
            "    fields:\n",
            "      id:\n",
            "        type: uuid\n",
            "        primary_key: true\n",
            "        require: true\n",
            "    access:\n",
            "      list: { public: true }\n",
            "      read: { public: true }\n",
            "      create: { public: true }\n",
            "      update: { public: true }\n",
            "      delete: { public: true }\n",
        ),
    )
    .unwrap();

    let diagnostics = compile_project(temporary.path()).unwrap_err();
    assert_eq!(diagnostics[0].code, "AS1012");
    assert_eq!(diagnostics[0].primary.span.line, 8);
}

#[test]
fn rejects_revision_field_reserved_for_concurrency() {
    let temporary = tempfile::tempdir().unwrap();
    copy_fixture(&fixture(), temporary.path());
    let spec_path = temporary.path().join("spec/project.yaml");
    let spec = fs::read_to_string(&spec_path).unwrap();
    fs::write(
        &spec_path,
        spec.replace("      name:\n", "      revision:\n"),
    )
    .unwrap();

    let diagnostics = compile_project(temporary.path()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS2012")
    );
}

fn copy_fixture(source: &Path, destination: &Path) {
    fs::create_dir_all(destination.join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), destination.join(relative)).unwrap();
    }
}

fn add_module_declaration(project: &Path, manifest: &str) {
    let root_file = project.join("appstruct.yaml");
    let source = fs::read_to_string(&root_file).unwrap();
    fs::write(
        root_file,
        format!("{source}\nmodule_manifests:\n  - {manifest}\n"),
    )
    .unwrap();
}
