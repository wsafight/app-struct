#[allow(dead_code)]
mod support;

use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use std::{fs, path::Path};

#[test]
fn generated_auth_limits_are_shared_and_recover_after_the_window() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let project = tempfile::tempdir().unwrap();
    for artifact in artifacts {
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
    fs::create_dir(backend.join("tests")).unwrap();
    fs::write(
        backend.join("tests/auth_limits.rs"),
        include_str!("auth_rate_limit/runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&manifest, "auth_limits");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
