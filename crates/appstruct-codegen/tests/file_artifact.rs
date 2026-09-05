mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use appstruct_ir::FileProviderIr;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check};

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
    assert!(file.contains("put_with_connection<C: ConnectionTrait>"));
    assert!(file.contains("load_metadata<C: ConnectionTrait>"));
    let delete = &file[file.find("pub async fn delete").expect("delete handler")..];
    let metadata = delete.find("DELETE FROM").expect("metadata delete");
    let storage = delete.find("self.provider.delete").expect("storage delete");
    assert!(
        storage < metadata,
        "file delete must remove storage before metadata so failures remain retryable"
    );
    let extensions = artifact_text(&artifacts, "backend/src/extensions.rs");
    assert!(extensions.contains("pub async fn put_file"));
    assert!(extensions.contains("pub async fn get_file"));
    assert!(extensions.contains("pub async fn delete_file"));
    assert!(extensions.contains("put_with_connection("));
    assert!(extensions.contains("delete_with_connection(self"));
    let manifest = artifact_text(&artifacts, "backend/Cargo.toml");
    assert!(manifest.contains("object_store"));
    assert_eq!(
        manifest.contains("features = [\"aws\"]"),
        provider == FileProviderIr::S3
    );
    assert!(manifest.contains("infer = \"=0.19.0\""));
    let admin = artifact_text(&artifacts, "backend/src/auth/admin_storage.rs");
    assert!(admin.contains("/api/admin/files"));
    assert!(admin.contains("total_bytes"));
    let admin_pages = artifact_text(&artifacts, "web/src/auth/AdminStoragePages.tsx");
    assert!(admin_pages.contains("AdminFileDetailPage"));
    let openapi: serde_json::Value =
        serde_json::from_str(artifact_text(&artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/admin/files"]["get"].is_object());
    assert!(openapi["paths"]["/api/admin/files/{id}"]["get"].is_object());

    let manifest = root.join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, true);
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
