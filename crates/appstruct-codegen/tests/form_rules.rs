#[allow(dead_code)]
mod support;
use std::{fs, path::Path};

#[test]
fn generated_form_rules_validate_complete_candidates() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/form-rules-project");
    let ir = appstruct_compiler::compile_project(&fixture).unwrap();
    let project = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = project.path().join(artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = project.path().join("backend");
    let manifest = backend.join("Cargo.toml");
    let mut manifest_source = fs::read_to_string(&manifest).unwrap();
    manifest_source.push_str(
        "\n[dev-dependencies]\ntower = { version = \"=0.5.3\", features = [\"util\"] }\n",
    );
    fs::write(manifest, manifest_source).unwrap();
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(
        backend.join("tests/form_rules.rs"),
        include_str!("form_rules/runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&backend.join("Cargo.toml"), "form_rules");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
