#[allow(dead_code)]
mod support;

use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use std::{fs, path::Path};

#[test]
fn generated_auth_security_behaviors_hold() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let mut ir = compile_project(&fixture).unwrap();
    ir.auth.oauth_enabled = true;
    ir.auth.oauth_providers = vec!["oidc".to_owned(), "google".to_owned()];
    let project = tempfile::tempdir().unwrap();
    for artifact in plan(&ir).unwrap() {
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
        backend.join("tests/auth_security.rs"),
        include_str!("auth_security_behavior/runtime.rs"),
    )
    .unwrap();
    let output = support::cargo_test(&manifest, "auth_security");
    if !output.status.success() {
        let generated = project.keep();
        panic!(
            "{}\n{}\ngenerated crate retained at {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            generated.display()
        );
    }
}
