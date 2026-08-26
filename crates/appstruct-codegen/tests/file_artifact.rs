use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use appstruct_ir::FileProviderIr;
use std::{fs, path::Path, process::Command};

#[test]
fn file_contract_generates_compilable_local_and_s3_backends() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-file-project");
    let ir = compile_project(&fixture).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    assert_provider(
        &ir,
        FileProviderIr::Local,
        "LocalFileSystem",
        temporary.path(),
    );
    assert_provider(&ir, FileProviderIr::S3, "AmazonS3Builder", temporary.path());
}

fn assert_provider(
    ir: &appstruct_ir::AppIr,
    provider: FileProviderIr,
    marker: &str,
    temporary: &Path,
) {
    let mut ir = ir.clone();
    ir.file.provider = provider;
    let artifacts = plan(&ir).unwrap();
    let name = match provider {
        FileProviderIr::Local => "local",
        FileProviderIr::S3 => "s3",
    };
    let root = temporary.join(name);
    write_artifacts(&root, &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_files"));
    assert!(sql.contains("\"object_key\" TEXT NOT NULL UNIQUE"));
    assert!(sql.contains("FOREIGN KEY (\"tenant_id\")"));
    let file = artifact_text(&artifacts, "backend/src/file.rs");
    assert!(file.contains(marker));
    assert!(file.contains("file key must be a safe relative path"));
    assert!(file.contains("stored object checksum does not match metadata"));
    assert!(file.contains("tenant_id IS NOT DISTINCT FROM"));
    let extensions = artifact_text(&artifacts, "backend/src/extensions.rs");
    assert!(extensions.contains("pub async fn put_file"));
    assert!(extensions.contains("pub async fn get_file"));
    assert!(extensions.contains("pub async fn delete_file"));
    let manifest = artifact_text(&artifacts, "backend/Cargo.toml");
    assert!(manifest.contains("object_store"));
    assert_eq!(
        manifest.contains("features = [\"aws\"]"),
        provider == FileProviderIr::S3
    );
    assert!(manifest.contains("infer = \"=0.19.0\""));

    let checked = cargo_check(
        &root.join("generated/backend/Cargo.toml"),
        &temporary.join("target"),
    );
    assert!(
        checked.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

fn write_artifacts(root: &Path, artifacts: &[Artifact]) {
    for artifact in artifacts {
        let destination = root.join("generated").join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &artifact.content).unwrap();
    }
}

fn cargo_check(manifest: &Path, target: &Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "--quiet", "--manifest-path"])
        .arg(manifest)
        .arg("--lib")
        .env("CARGO_TARGET_DIR", target)
        .output()
        .unwrap()
}
