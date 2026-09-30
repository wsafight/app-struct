use appstruct_compiler::compile_project;
use appstruct_ir::{FrameOptionsIr, HstsIr, ReferrerPolicyIr};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn project(appstruct: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("spec")).unwrap();
    std::fs::write(root.path().join("appstruct.yaml"), appstruct).unwrap();
    std::fs::write(
        root.path().join("spec/main.yaml"),
        "domain: main\nentities:\n  Note:\n    table: notes\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n        generated: uuid_v7\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n",
    )
    .unwrap();
    root
}

fn with_server(server: &str) -> String {
    format!(
        "version: 1\napp:\n  name: demo\ndatabase:\n  provider: postgres\n  dev:\n    mode: external\n{server}includes:\n  - spec/main.yaml\n"
    )
}

fn diagnostics(appstruct: &str) -> Vec<String> {
    let root = project(appstruct);
    match compile_project(root.path()) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics.into_iter().map(|d| d.code).collect(),
    }
}

#[test]
fn absent_server_block_uses_safe_defaults() {
    let ir = compile_project(&fixture("m0-project")).unwrap();
    let headers = &ir.server.security_headers;
    assert!(headers.enabled);
    assert_eq!(headers.hsts, HstsIr::Auto);
    assert_eq!(headers.frame_options, FrameOptionsIr::Deny);
    assert_eq!(
        headers.referrer_policy,
        ReferrerPolicyIr::StrictOriginWhenCrossOrigin
    );
    assert!(headers.content_security_policy.is_none());
    assert!(headers.custom.is_empty());
}

#[test]
fn server_block_is_decoded_from_the_spec() {
    let ir = compile_project(&fixture("m8-server-project")).unwrap();
    let headers = &ir.server.security_headers;
    assert!(headers.enabled);
    assert_eq!(headers.hsts, HstsIr::Auto);
    assert_eq!(headers.frame_options, FrameOptionsIr::SameOrigin);
    assert_eq!(headers.referrer_policy, ReferrerPolicyIr::NoReferrer);
    assert_eq!(
        headers.content_security_policy.as_deref(),
        Some("default-src 'self'")
    );
    assert_eq!(
        headers.custom.get("x-robots-tag").map(String::as_str),
        Some("noindex")
    );
}

#[test]
fn hsts_accepts_auto_off_and_explicit_max_age() {
    for (value, expected) in [
        ("auto", HstsIr::Auto),
        ("off", HstsIr::Off),
        ("0", HstsIr::MaxAge(0)),
        ("63072000", HstsIr::MaxAge(63_072_000)),
    ] {
        let root = project(&with_server(&format!(
            "server:\n  security_headers:\n    hsts: {value}\n"
        )));
        let ir = compile_project(root.path()).unwrap();
        assert_eq!(ir.server.security_headers.hsts, expected, "hsts: {value}");
    }
}

#[test]
fn content_security_policy_distinguishes_yaml_null_from_a_string() {
    let root = project(&with_server(
        "server:\n  security_headers:\n    content_security_policy: null\n",
    ));
    let ir = compile_project(root.path()).unwrap();
    assert!(ir.server.security_headers.content_security_policy.is_none());

    let root = project(&with_server(
        "server:\n  security_headers:\n    content_security_policy: \"null\"\n",
    ));
    let ir = compile_project(root.path()).unwrap();
    assert_eq!(
        ir.server
            .security_headers
            .content_security_policy
            .as_deref(),
        Some("null")
    );
}

#[test]
fn invalid_security_header_values_are_rejected() {
    for server in [
        "server:\n  security_headers:\n    hsts: sometimes\n",
        "server:\n  security_headers:\n    frame_options: sometimes\n",
        "server:\n  security_headers:\n    referrer_policy: sometimes\n",
        "server:\n  security_headers:\n    custom:\n      X-Upper: value\n",
        "server:\n  security_headers:\n    custom:\n      bad_header: value\n",
        "server:\n  security_headers:\n    custom:\n      x-tab: \"a\\u0001b\"\n",
        "server:\n  security_headers:\n    content_security_policy: \"héllo\"\n",
    ] {
        assert_eq!(
            diagnostics(&with_server(server)),
            vec!["AS1013"],
            "expected AS1013 for:\n{server}"
        );
    }
}

#[test]
fn unknown_server_keys_are_rejected() {
    for server in [
        "server:\n  unexpected: true\n",
        "server:\n  security_headers:\n    unexpected: true\n",
    ] {
        assert_eq!(
            diagnostics(&with_server(server)),
            vec!["AS1012"],
            "expected AS1012 for:\n{server}"
        );
    }
}

#[test]
fn disabling_security_headers_with_configuration_warns() {
    let root = project(&with_server(
        "server:\n  security_headers:\n    enabled: false\n    content_security_policy: \"default-src 'self'\"\n",
    ));
    let report = appstruct_compiler::compile_project_report(root.path()).unwrap();
    assert!(
        report.diagnostics.iter().any(|d| d.code == "AS3108"),
        "expected AS3108 in {:?}",
        report.diagnostics
    );
    assert!(!report.ir.server.security_headers.enabled);
}
