use appstruct_codegen::{check_user_symbols, plan, required_symbols};
use appstruct_compiler::compile_project;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

#[test]
fn required_symbols_are_derived_from_pages_and_field_components() {
    let ir = compile_project(&fixture("m3-project")).unwrap();
    let required = required_symbols(&ir);
    assert!(required.page_components.contains("ProjectDashboard"));
    assert!(required.field_components.contains("ProjectMetadataEditor"));
}

#[test]
fn fixtures_without_custom_components_require_nothing() {
    let ir = compile_project(&fixture("m0-project")).unwrap();
    assert!(required_symbols(&ir).is_empty());
    assert!(
        check_user_symbols(&fixture("m0-project"), &ir)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn provided_symbols_satisfy_the_contract() {
    let ir = compile_project(&fixture("m3-project")).unwrap();
    let diagnostics = check_user_symbols(&fixture("m3-project"), &ir).unwrap();
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn missing_page_component_is_reported() {
    let ir = compile_project(&fixture("m3-missing-symbol")).unwrap();
    let diagnostics = check_user_symbols(&fixture("m3-missing-symbol"), &ir).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "AS3106");
    assert!(diagnostics[0].message.contains("ProjectDashboard"));
    assert_eq!(diagnostics[0].primary.span.file, "app/web/registry.tsx");
}

#[test]
fn missing_field_component_is_reported() {
    let project = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m3-project"), project.path());
    fs::write(
        project.path().join("app/web/registry.tsx"),
        "export const registry = { fields: {}, pages: { ProjectDashboard } };\n",
    )
    .unwrap();
    let ir = compile_project(project.path()).unwrap();
    let diagnostics = check_user_symbols(project.path(), &ir).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "AS3107");
    assert!(diagnostics[0].message.contains("ProjectMetadataEditor"));
}

#[test]
fn absent_app_web_directory_reports_every_referenced_symbol() {
    let project = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m3-project"), project.path());
    fs::remove_dir_all(project.path().join("app/web")).unwrap();
    let ir = compile_project(project.path()).unwrap();
    let diagnostics = check_user_symbols(project.path(), &ir).unwrap();
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
}

#[test]
fn unparsable_registry_makes_the_fast_check_inconclusive() {
    let project = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m3-project"), project.path());
    fs::write(
        project.path().join("app/web/registry.tsx"),
        "export const { = broken\n",
    )
    .unwrap();
    let ir = compile_project(project.path()).unwrap();
    let diagnostics = check_user_symbols(project.path(), &ir).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unrelated_exports_do_not_satisfy_missing_registry_members() {
    let project = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m3-project"), project.path());
    fs::write(
        project.path().join("app/web/registry.tsx"),
        concat!(
            "export function ProjectDashboard() { return null; }\n",
            "export const registry = { fields: { ProjectMetadataEditor }, pages: {} };\n",
        ),
    )
    .unwrap();
    let ir = compile_project(project.path()).unwrap();
    let diagnostics = check_user_symbols(project.path(), &ir).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "AS3106");
}

#[test]
fn field_registry_members_do_not_satisfy_page_requirements() {
    let project = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m3-project"), project.path());
    fs::write(
        project.path().join("app/web/registry.tsx"),
        "export const registry = { fields: { ProjectMetadataEditor, ProjectDashboard }, pages: {} };\n",
    )
    .unwrap();
    let ir = compile_project(project.path()).unwrap();
    let diagnostics = check_user_symbols(project.path(), &ir).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "AS3106");
}

#[test]
fn reexporting_sources_make_the_check_inconclusive() {
    let project = tempfile::tempdir().unwrap();
    copy_directory(&fixture("m3-project"), project.path());
    fs::write(
        project.path().join("app/web/registry.tsx"),
        "export * from \"./components\";\n",
    )
    .unwrap();
    let ir = compile_project(project.path()).unwrap();
    let diagnostics = check_user_symbols(project.path(), &ir).unwrap();
    assert!(
        diagnostics.is_empty(),
        "a re-export cannot be resolved, so nothing may be reported: {diagnostics:?}"
    );
}

#[test]
fn symbol_check_does_not_change_generated_artifacts() {
    let ir = compile_project(&fixture("m3-project")).unwrap();
    let first = plan(&ir).unwrap();
    check_user_symbols(&fixture("m3-project"), &ir).unwrap();
    let second = plan(&ir).unwrap();
    assert_eq!(first, second);
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
