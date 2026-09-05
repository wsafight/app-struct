use appstruct_compiler::compile_project;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-report-project")
}

#[test]
fn lowers_report_contract_and_official_module() {
    let ir = compile_project(&fixture()).unwrap();
    assert!(ir.report.enabled);
    assert_eq!(ir.report.queue, "reports");
    assert_eq!(ir.report.max_input_bytes, 262_144);
    assert_eq!(ir.report.retention_days, 30);
    assert_eq!(ir.report.reader_roles, ["auditor", "admin"]);
    let template = &ir.report.templates[0];
    assert_eq!(template.name, "order-summary");
    assert_eq!(template.version, 1);
    assert_eq!(template.document_type, "pdf");
    assert!(template.artifact_digest.starts_with("sha256:"));
    let module = ir
        .modules
        .iter()
        .find(|module| module.name == "appstruct/report")
        .unwrap();
    assert_eq!(module.provides, ["report.render"]);
    assert_eq!(
        module.requires,
        ["auth.identity", "file.storage", "jobs.outbox"]
    );
}

#[test]
fn rejects_missing_dependencies_queue_and_invalid_template_schema() {
    let cases = [
        (
            "  auth:\n    enabled: true",
            "  auth:\n    enabled: false",
            "AS3093",
        ),
        ("    queue: reports", "    queue: missing", "AS3094"),
        (
            "'{\"type\":\"object\",\"required\":[\"order_id\"],\"properties\":{\"order_id\":{\"type\":\"string\",\"format\":\"uuid\"}},\"additionalProperties\":false}'",
            "'{oops'",
            "AS3099",
        ),
    ];
    for (old, new, code) in cases {
        let temporary = copy_fixture();
        replace(&temporary.path().join("appstruct.yaml"), old, new);
        let diagnostics = compile_project(temporary.path()).unwrap_err();
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "expected {code}, got {diagnostics:#?}",
        );
    }
}

fn copy_fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/order.yaml"] {
        fs::copy(fixture().join(relative), temporary.path().join(relative)).unwrap();
    }
    temporary
}

fn replace(path: &Path, old: &str, new: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains(old), "fixture does not contain {old}");
    fs::write(path, source.replacen(old, new, 1)).unwrap();
}
