#[allow(dead_code)]
mod support;

use std::{fs, path::Path};

#[test]
fn generated_file_and_audit_lifecycle() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/operations-demo");
    let ir = appstruct_compiler::compile_project(&fixture).unwrap();
    let project = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = project.path().join(&artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = project.path().join("backend");
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(
        backend.join("tests/lifecycle.rs"),
        include_str!("data_lifecycle/runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&backend.join("Cargo.toml"), "lifecycle");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
