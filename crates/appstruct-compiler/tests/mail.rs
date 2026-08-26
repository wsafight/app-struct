use appstruct_compiler::compile_project;
use appstruct_ir::MailProviderIr;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-mail-project")
}

#[test]
fn lowers_mail_provider_and_sorted_templates() {
    let ir = compile_project(&fixture()).unwrap();
    assert!(ir.mail.enabled);
    assert_eq!(ir.mail.provider, MailProviderIr::Capture);
    assert_eq!(
        ir.mail
            .templates
            .iter()
            .map(|template| template.name.as_str())
            .collect::<Vec<_>>(),
        ["project-created", "welcome"]
    );
    assert_eq!(
        ir.mail.templates[0].html.as_deref(),
        Some("<p>Hello {{ recipient_name }}, your project is ready.</p>")
    );
}

#[test]
fn mail_requires_provider_sender_and_templates() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    provider: capture",
        "    provider: postal",
    );
    assert_diagnostic(temporary.path(), "AS3041");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    from: \"AppStruct <notifications@example.com>\"",
        "    from: invalid",
    );
    assert_diagnostic(temporary.path(), "AS3042");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "    templates:\n      project-created:\n        subject: \"Project {{ project_name }} created\"\n        text: \"Hello {{ recipient_name }}, your project is ready.\"\n        html: \"<p>Hello {{ recipient_name }}, your project is ready.</p>\"\n      welcome:\n        subject: \"Welcome, {{ recipient_name }}\"\n        text: \"Your AppStruct workspace is ready.\"",
        "    templates: {}",
    );
    assert_diagnostic(temporary.path(), "AS3043");
}

#[test]
fn mail_rejects_invalid_template_name_and_syntax() {
    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "      project-created:",
        "      ProjectCreated:",
    );
    assert_diagnostic(temporary.path(), "AS3044");

    let temporary = copied_fixture();
    replace(
        &temporary.path().join("appstruct.yaml"),
        "Project {{ project_name }} created",
        "Project {{ project_name created",
    );
    assert_diagnostic(temporary.path(), "AS3046");
}

fn copied_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(fixture().join(relative), temporary.path().join(relative)).unwrap();
    }
    temporary
}

fn replace(path: &Path, old: &str, new: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains(old));
    fs::write(path, source.replacen(old, new, 1)).unwrap();
}

fn assert_diagnostic(project: &Path, code: &str) {
    let diagnostics = compile_project(project).unwrap_err();
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "expected {code}, got {diagnostics:#?}"
    );
}
