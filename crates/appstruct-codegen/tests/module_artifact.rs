use appstruct_codegen::{ArtifactKind, plan};
use appstruct_compiler::{compile_project, updated_project_lock};
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
    fs::write(
        temporary.path().join("appstruct.lock"),
        updated_project_lock(temporary.path()).unwrap(),
    )
    .unwrap();

    let mut ir = compile_project(temporary.path()).unwrap();
    let artifacts = plan(&ir).unwrap();
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

    let local = ir
        .modules
        .iter_mut()
        .find(|module| module.name == "local/example")
        .unwrap();
    local.artifacts[0].content.push_str("tampered\n");
    let error = plan(&ir).unwrap_err();
    assert!(error.to_string().contains("does not match its SHA-256"));
}

#[test]
fn rejects_official_provenance_invalid_digests_and_byte_length() {
    use sha2::{Digest, Sha256};

    let mut ir: appstruct_ir::AppIr =
        serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
    ir.modules[0].origin = appstruct_ir::ModuleOrigin::Official;
    ir.modules[0].manifest_path = Some("modules/example/module.toml".to_owned());
    assert!(
        plan(&ir)
            .unwrap_err()
            .to_string()
            .contains("official module")
    );
    ir.modules[0].origin = appstruct_ir::ModuleOrigin::Local;
    ir.modules[0].name = "local/example".to_owned();
    ir.modules[0].manifest_path = Some("modules/example/module.toml".to_owned());
    ir.modules[0].content_sha256 = Some(format!("sha256:{:x}", Sha256::digest(b"manifest")));
    ir.modules[0].artifacts = vec![appstruct_ir::ModuleArtifactIr {
        path: "docs/README.md".to_owned(),
        source: Some("modules/example/assets/README.md".to_owned()),
        sha256: "sha256:deadbeef".to_owned(),
        byte_len: 2,
        content: "ok".to_owned(),
    }];
    let digest_error = plan(&ir).unwrap_err().to_string();
    assert!(
        digest_error.contains("SHA-256") || digest_error.contains("sha256"),
        "{digest_error}"
    );
    ir.modules[0].artifacts[0].sha256 = format!("sha256:{:x}", Sha256::digest(b"ok"));
    ir.modules[0].artifacts[0].byte_len = 99;
    assert!(plan(&ir).unwrap_err().to_string().contains("byte length"));
}

fn copy_fixture(source: &Path, destination: &Path) {
    fs::create_dir_all(destination.join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(source.join(relative), destination.join(relative)).unwrap();
    }
}
