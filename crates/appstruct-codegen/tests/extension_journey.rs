#[allow(dead_code)]
mod support;
use std::{fs, path::Path};

#[test]
fn user_hook_failure_and_request_id_journey() {
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/business-extension-demo");
    let ir = appstruct_compiler::compile_project(&example).unwrap();
    let project = tempfile::tempdir().unwrap();
    for artifact in appstruct_codegen::plan(&ir).unwrap() {
        let path = project.path().join(&artifact.relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, artifact.content).unwrap();
    }
    let backend = project.path().join("backend");
    let manifest = backend.join("Cargo.toml");
    let mut source = fs::read_to_string(&manifest).unwrap();
    source.push_str(
        "\n[dev-dependencies]\ntower = { version = \"=0.5.3\", features = [\"util\"] }\n",
    );
    fs::write(&manifest, source).unwrap();
    fs::create_dir_all(backend.join("tests/example")).unwrap();
    fs::copy(
        example.join("app/backend/src/lib.rs"),
        backend.join("tests/example/mod.rs"),
    )
    .unwrap();
    fs::write(
        backend.join("tests/journey.rs"),
        include_str!("extension_journey/runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&manifest, "journey");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
