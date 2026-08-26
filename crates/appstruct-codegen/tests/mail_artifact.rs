mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use appstruct_ir::MailProviderIr;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check};

#[test]
fn mail_contract_generates_a_compilable_backend() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-mail-project");
    let ir = compile_project(&fixture).unwrap();
    let artifacts = plan(&ir).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    let sql = artifact_text(&artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_mail_deliveries"));
    assert!(sql.contains("FOREIGN KEY (\"tenant_id\")"));
    let mail = artifact_text(&artifacts, "backend/src/mail.rs");
    assert!(mail.contains("pub trait MailProvider"));
    assert!(mail.contains("capture mail provider is forbidden in production"));
    assert!(mail.contains("Project {{ project_name }} created"));
    assert!(!mail.contains("SmtpProvider"));
    assert!(!mail.contains("ResendProvider"));
    let extensions = artifact_text(&artifacts, "backend/src/extensions.rs");
    assert!(extensions.contains("pub async fn send_mail"));
    let manifest = artifact_text(&artifacts, "backend/Cargo.toml");
    assert!(manifest.contains("minijinja = \"=2.12.0\""));
    assert!(!manifest.contains("reqwest"));

    let manifest_path = temporary.path().join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest_path);
    let checked = cargo_check(&manifest_path, true);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    assert_provider_compiles(&ir, MailProviderIr::Smtp, "SmtpProvider", temporary.path());
    assert_provider_compiles(
        &ir,
        MailProviderIr::Resend,
        "https://api.resend.com/emails",
        temporary.path(),
    );
}

fn assert_provider_compiles(
    ir: &appstruct_ir::AppIr,
    provider: MailProviderIr,
    marker: &str,
    temporary: &Path,
) {
    let mut ir = ir.clone();
    ir.mail.provider = provider;
    let artifacts = plan(&ir).unwrap();
    let name = match provider {
        MailProviderIr::Capture => "capture",
        MailProviderIr::Smtp => "smtp",
        MailProviderIr::Resend => "resend",
    };
    let root = temporary.join(name);
    write_artifacts(&root, &artifacts);
    assert!(artifact_text(&artifacts, "backend/src/mail.rs").contains(marker));
    let manifest = artifact_text(&artifacts, "backend/Cargo.toml");
    assert_eq!(
        manifest.contains("reqwest"),
        provider == MailProviderIr::Resend
    );
    let manifest_path = root.join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest_path);
    let checked = cargo_check(&manifest_path, true);
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
