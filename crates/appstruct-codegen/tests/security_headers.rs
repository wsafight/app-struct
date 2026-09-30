mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use appstruct_ir::{AppIr, FrameOptionsIr, HstsIr, ReferrerPolicyIr, SecurityHeadersIr, ServerIr};
use std::fs;
use std::path::Path;
use support::{assert_rustfmt, cargo_check};

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn generated_backend(project: &Path) -> String {
    let ir = compile_project(project).unwrap();
    let artifacts = plan(&ir).unwrap();
    artifact_text(&artifacts, "backend/src/lib.rs").to_owned()
}

fn with_headers(mutate: impl FnOnce(&mut SecurityHeadersIr)) -> String {
    let mut ir = compile_project(&fixture("m8-server-project")).unwrap();
    mutate(&mut ir.server.security_headers);
    let artifacts = plan(&ir).unwrap();
    artifact_text(&artifacts, "backend/src/lib.rs").to_owned()
}

#[test]
fn configured_security_headers_are_emitted_in_the_router() {
    let backend = generated_backend(&fixture("m8-server-project"));
    assert!(backend.contains("fn apply_security_headers"));
    assert!(backend.contains("X_CONTENT_TYPE_OPTIONS"));
    assert!(backend.contains("\"nosniff\""));
    assert!(backend.contains("X_FRAME_OPTIONS"));
    assert!(backend.contains("\"SAMEORIGIN\""));
    assert!(backend.contains("REFERRER_POLICY"));
    assert!(backend.contains("\"no-referrer\""));
    assert!(backend.contains("CONTENT_SECURITY_POLICY"));
    assert!(backend.contains("\"default-src 'self'\""));
    assert!(backend.contains("HeaderName::from_static(\"x-robots-tag\")"));
    assert!(backend.contains("\"noindex\""));
    assert!(backend.contains("SetResponseHeaderLayer::if_not_present"));
    assert!(backend.contains("STRICT_TRANSPORT_SECURITY"));
    assert!(backend.contains("max-age=31536000; includeSubDomains"));
    assert!(backend.contains("APPSTRUCT_ENV"));
    assert!(backend.contains("let router = apply_security_headers(router);"));
}

#[test]
fn security_headers_are_enabled_by_default() {
    // m0 declares no `server:` block at all, so the IR default must still emit headers.
    let ir: AppIr = compile_project(&fixture("m0-project")).unwrap();
    assert_eq!(ir.server, ServerIr::default());
    assert!(ir.server.security_headers.enabled);
    let backend = generated_backend(&fixture("m0-project"));
    assert!(backend.contains("X_CONTENT_TYPE_OPTIONS"));
    assert!(backend.contains("X_FRAME_OPTIONS"));
    assert!(backend.contains("REFERRER_POLICY"));
    assert!(backend.contains("fn apply_security_headers"));
}

#[test]
fn disabling_security_headers_keeps_the_router_passthrough() {
    let backend = with_headers(|headers| headers.enabled = false);
    assert!(backend.contains("fn apply_security_headers"));
    assert!(!backend.contains("X_CONTENT_TYPE_OPTIONS"));
    assert!(!backend.contains("SetResponseHeaderLayer"));
    assert!(!backend.contains("STRICT_TRANSPORT_SECURITY"));
}

#[test]
fn off_policies_omit_only_their_own_headers() {
    let backend = with_headers(|headers| {
        headers.frame_options = FrameOptionsIr::Off;
        headers.referrer_policy = ReferrerPolicyIr::Off;
        headers.hsts = HstsIr::Off;
        headers.content_security_policy = None;
    });
    assert!(backend.contains("X_CONTENT_TYPE_OPTIONS"));
    assert!(!backend.contains("X_FRAME_OPTIONS"));
    assert!(!backend.contains("REFERRER_POLICY"));
    assert!(!backend.contains("STRICT_TRANSPORT_SECURITY"));
    assert!(!backend.contains("CONTENT_SECURITY_POLICY"));
}

#[test]
fn explicit_hsts_max_age_is_emitted_without_environment_check() {
    let backend = with_headers(|headers| headers.hsts = HstsIr::MaxAge(60));
    assert!(backend.contains("max-age=60; includeSubDomains"));
    assert!(!backend.contains("APPSTRUCT_ENV"));
}

#[test]
fn custom_headers_are_emitted_in_sorted_order() {
    let backend = with_headers(|headers| {
        headers.custom.clear();
        headers.custom.insert("z-last".to_owned(), "z".to_owned());
        headers.custom.insert("a-first".to_owned(), "a".to_owned());
    });
    let first = backend.find("a-first").unwrap();
    let last = backend.find("z-last").unwrap();
    assert!(
        first < last,
        "custom headers must be sorted for determinism"
    );
}

#[test]
fn invalid_custom_headers_are_rejected_before_generation() {
    let mut ir = compile_project(&fixture("m8-server-project")).unwrap();
    ir.server.security_headers.custom.clear();
    ir.server
        .security_headers
        .custom
        .insert("X-Robots-Tag".to_owned(), "noindex".to_owned());

    let error = plan(&ir).unwrap_err();
    assert!(error.to_string().contains("X-Robots-Tag"), "{error}");
}

#[test]
fn generated_backend_declares_the_set_header_feature() {
    let ir = compile_project(&fixture("m8-server-project")).unwrap();
    let artifacts = plan(&ir).unwrap();
    let manifest = artifact_text(&artifacts, "backend/Cargo.toml");
    assert!(manifest.contains("\"set-header\""));
}

#[test]
fn hardened_fixture_generates_a_compilable_backend() {
    let ir = compile_project(&fixture("m8-server-project")).unwrap();
    let artifacts = plan(&ir).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    for artifact in &artifacts {
        let destination = temporary
            .path()
            .join("generated")
            .join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &artifact.content).unwrap();
    }
    let app_backend = temporary.path().join("app/backend");
    fs::create_dir_all(app_backend.join("src")).unwrap();
    fs::write(
        app_backend.join("Cargo.toml"),
        concat!(
            "[package]\nname = \"appstruct-app-backend\"\nversion = \"0.0.0\"\n",
            "edition = \"2024\"\n\n[dependencies]\n",
            "appstruct-generated-backend = { path = \"../../generated/backend\" }\n",
        ),
    )
    .unwrap();
    fs::write(
        app_backend.join("src/lib.rs"),
        concat!(
            "use appstruct_generated_backend::AppExtensions;\n",
            "pub fn extensions() -> AppExtensions { AppExtensions::builder().build() }\n",
        ),
    )
    .unwrap();
    let manifest = temporary.path().join("generated/server/Cargo.toml");
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, false);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}
