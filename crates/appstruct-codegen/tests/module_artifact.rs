use appstruct_codegen::{ArtifactKind, plan};
use appstruct_compiler::compile_project;
use std::fs;
use std::path::Path;

#[test]
fn isolates_local_artifacts_and_generates_a_noop_runtime_starter() {
    let temporary = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    copy_fixture(&fixture, temporary.path());
    let root_path = temporary.path().join("appstruct.yaml");
    let root = fs::read_to_string(&root_path).unwrap();
    fs::write(
        root_path,
        format!("{root}\nmodule_manifests:\n  - modules/example/module.toml\n"),
    )
    .unwrap();
    fs::create_dir_all(temporary.path().join("modules/example/assets")).unwrap();
    fs::write(
        temporary.path().join("modules/example/module.toml"),
        concat!(
            "api_version = 1\n",
            "name = \"local/example\"\n",
            "version = \"0.1.0\"\n",
            "requires = [\"auth.identity\"]\n\n",
            "[[artifacts]]\n",
            "path = \"docs/README.md\"\n",
            "source = \"assets/README.md\"\n",
        ),
    )
    .unwrap();
    fs::write(
        temporary.path().join("modules/example/assets/README.md"),
        "local content\n",
    )
    .unwrap();

    let artifacts = plan(&compile_project(temporary.path()).unwrap()).unwrap();
    let module_artifact = artifacts
        .iter()
        .find(|artifact| {
            artifact.relative_path == Path::new("modules/local+example/docs/README.md")
        })
        .unwrap();
    assert_eq!(module_artifact.kind, ArtifactKind::Module);
    assert_eq!(module_artifact.content, b"local content\n");

    let library = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new("backend/src/lib.rs"))
        .unwrap();
    let library = std::str::from_utf8(&library.content).unwrap();
    assert!(library.contains("Local2"));
    assert!(library.contains("Self::Local2 => Ok(None)"));
}

fn copy_fixture(source: &Path, destination: &Path) {
    fs::create_dir_all(destination.join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), destination.join(relative)).unwrap();
    }
}
