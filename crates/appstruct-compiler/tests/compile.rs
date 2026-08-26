use appstruct_compiler::compile_project;
use appstruct_ir::{OperationTypeIr, to_canonical_json};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

#[test]
fn compiles_fixture_to_canonical_golden() {
    let ir = compile_project(&fixture()).unwrap();
    let actual = to_canonical_json(&ir).unwrap();
    let expected = include_str!("../../../tests/golden/m0-app-ir.json");
    assert_eq!(actual, expected);
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

fn copy_fixture(source: &Path, destination: &Path) {
    fs::create_dir_all(destination.join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), destination.join(relative)).unwrap();
    }
}
