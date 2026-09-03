use super::{install_envelope, lifecycle, verification};
use appstruct_ir::ModuleOrigin;
use appstruct_module_sdk::{
    MODULE_API_VERSION, RegistryArtifact, RegistryEnvelope, RegistryPackage,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn installs_and_compiles_a_signed_remote_module_offline() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: registry-test\ndatabase:\n  provider: postgres\nincludes: []\n",
    )
    .unwrap();
    let signing = SigningKey::from_bytes(&[7_u8; 32]);
    let public_key = STANDARD.encode(signing.verifying_key().as_bytes());
    let artifact = b"# Signed module\n";
    let package = RegistryPackage {
        name: "vendor/analytics".to_owned(),
        version: "1.2.3".to_owned(),
        appstruct_version: env!("CARGO_PKG_VERSION").to_owned(),
        module_api_version: MODULE_API_VERSION,
        manifest: concat!(
            "api_version = 1\n",
            "name = \"vendor/analytics\"\n",
            "version = \"1.2.3\"\n",
            "provides = [\"analytics.events\"]\n\n",
            "[[artifacts]]\n",
            "path = \"docs/README.md\"\n",
            "source = \"assets/README.md\"\n",
        )
        .to_owned(),
        artifacts: vec![RegistryArtifact {
            source: "assets/README.md".to_owned(),
            content: STANDARD.encode(artifact),
            sha256: format!("sha256:{:x}", Sha256::digest(artifact)),
            byte_len: artifact.len() as u64,
        }],
    };
    let payload = serde_json::to_vec(&package).unwrap();
    let envelope = RegistryEnvelope {
        schema_version: 1,
        payload: STANDARD.encode(&payload),
        sha256: format!("sha256:{:x}", Sha256::digest(&payload)),
        signature: STANDARD.encode(signing.sign(&payload).to_bytes()),
    };
    install_envelope(
        project.path(),
        "https://registry.example.com",
        &public_key,
        "vendor/analytics",
        "1.2.3",
        &serde_json::to_vec(&envelope).unwrap(),
    )
    .unwrap();

    verification::verify(project.path(), None).unwrap();

    let ir = appstruct_compiler::compile_project(project.path()).unwrap();
    let module = ir
        .modules
        .iter()
        .find(|module| module.name == "vendor/analytics")
        .unwrap();
    assert_eq!(module.origin, ModuleOrigin::Remote);
    assert_eq!(module.artifacts[0].content, "# Signed module\n");

    let lock = super::read_lock(project.path()).unwrap();
    let manifest = project.path().join(&lock.modules[0].manifest_path);
    fs::write(
        manifest.parent().unwrap().join("assets/README.md"),
        "tampered\n",
    )
    .unwrap();
    let verification_error =
        verification::verify(project.path(), Some("vendor/analytics")).unwrap_err();
    assert!(verification_error.contains("cached artifact"));
    let diagnostics = appstruct_compiler::compile_project(project.path()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AS3090")
    );

    let cache = manifest.parent().unwrap().to_owned();
    lifecycle::uninstall(project.path(), "vendor/analytics").unwrap();
    assert!(super::read_lock(project.path()).unwrap().modules.is_empty());
    assert!(!cache.exists());
}

#[test]
fn rejects_a_tampered_registry_signature() {
    let project = tempfile::tempdir().unwrap();
    let signing = SigningKey::from_bytes(&[9_u8; 32]);
    let envelope = RegistryEnvelope {
        schema_version: 1,
        payload: STANDARD.encode(b"{}"),
        sha256: format!("sha256:{:x}", Sha256::digest(b"{}")),
        signature: STANDARD.encode([0_u8; 64]),
    };
    let error = install_envelope(
        project.path(),
        "https://registry.example.com",
        &STANDARD.encode(signing.verifying_key().as_bytes()),
        "vendor/analytics",
        "1.2.3",
        &serde_json::to_vec(&envelope).unwrap(),
    )
    .unwrap_err();
    assert!(error.contains("signature verification failed"));
}

#[test]
fn parse_reference_and_registry_urls_validate_shape() {
    assert_eq!(
        super::parse_reference("vendor/analytics@1.2.3").unwrap(),
        ("vendor/analytics", "1.2.3")
    );
    assert!(super::parse_reference("vendor/analytics").is_err());
    assert!(super::parse_reference("@1.2.3").is_err());
    assert!(super::validate_registry_url("https://registry.example.com").is_ok());
    assert!(super::validate_registry_url("http://127.0.0.1:8080").is_ok());
    assert!(super::validate_registry_url("http://example.com").is_err());
}

#[test]
fn list_and_run_cover_empty_locks_and_invalid_commands() {
    let project = tempfile::tempdir().unwrap();
    super::list(project.path()).unwrap();
    assert_eq!(
        super::run(project.path(), &super::ModuleCommand::List),
        std::process::ExitCode::SUCCESS
    );
    assert_ne!(
        super::run(
            project.path(),
            &super::ModuleCommand::Install {
                module: "bad".to_owned(),
                registry: "http://example.com".to_owned(),
                public_key: None,
            },
        ),
        std::process::ExitCode::SUCCESS
    );
    fs::write(
        project.path().join("appstruct.modules.lock"),
        "lock_version = 9\n",
    )
    .unwrap();
    assert!(super::list(project.path()).is_err());
}

#[test]
fn update_and_uninstall_require_an_installed_module() {
    let project = tempfile::tempdir().unwrap();
    assert!(
        lifecycle::update(project.path(), "vendor/analytics@1.0.0", None, None)
            .unwrap_err()
            .contains("not installed")
    );
    assert!(
        lifecycle::uninstall(project.path(), "vendor/analytics")
            .unwrap_err()
            .contains("not installed")
    );
    assert_ne!(
        super::run(
            project.path(),
            &super::ModuleCommand::Uninstall {
                module: "vendor/analytics".to_owned(),
            },
        ),
        std::process::ExitCode::SUCCESS
    );
    let _ = super::run(
        project.path(),
        &super::ModuleCommand::Verify { module: None },
    );
}
