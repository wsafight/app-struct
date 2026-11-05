use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn saas_product_demo_preserves_the_starter_and_extends_its_modules() {
    let temporary = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .current_dir(temporary.path())
        .args(["new", "saas-demo", "--template", "saas"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo");
    let starter = temporary.path().join("saas-demo");
    assert_starter_files_preserved(&starter, &expected, Path::new(""));
    let starter = appstruct_compiler::compile_project(&starter).unwrap();
    let product = appstruct_compiler::compile_project(&expected).unwrap();
    assert!(starter.auth.enabled && starter.tenant.enabled && starter.file.enabled);
    assert!(!starter.billing.enabled && !starter.report.enabled);
    assert!(product.auth.enabled && product.tenant.enabled && product.file.enabled);
    assert!(product.billing.enabled && product.report.enabled);
    assert_eq!(product.report.free_runs, Some(3));
    let extensions = fs::read_to_string(expected.join("app/backend/src/lib.rs")).unwrap();
    assert!(extensions.contains(".routes(approvals::router().merge(assistant::router()))"));
    assert!(extensions.contains(".job_handler(assistant::AiJobs::default())"));
    let proxy = fs::read_to_string(expected.join("deploy/nginx.conf")).unwrap();
    let streaming = proxy
        .split("location ~ ^/api/product/assistant/[^/]+/events$ {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    for directive in [
        "proxy_pass http://$appstruct_api$request_uri;",
        "proxy_buffering off;",
        "proxy_cache off;",
        "proxy_read_timeout 65s;",
    ] {
        assert!(streaming.contains(directive));
    }
}

fn assert_starter_files_preserved(actual: &Path, expected: &Path, relative: &Path) {
    for name in entries(actual) {
        let relative_path = relative.join(&name);
        if matches!(
            relative_path.to_str(),
            Some(
                "appstruct.yaml"
                    | "README.md"
                    | ".env.example"
                    | ".gitignore"
                    | "app/backend/Cargo.toml"
                    | "app/backend/src/lib.rs"
                    | "deploy/nginx.conf"
            )
        ) {
            continue;
        }
        let actual_path = actual.join(&name);
        let expected_path = expected.join(name);
        if actual_path.is_dir() {
            assert!(expected_path.is_dir());
            assert_starter_files_preserved(&actual_path, &expected_path, &relative_path);
        } else {
            assert_eq!(
                fs::read(&actual_path).unwrap(),
                fs::read(&expected_path).unwrap(),
                "file bytes differ for {}",
                actual_path.display()
            );
        }
    }
}

fn entries(directory: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| PathBuf::from(entry.unwrap().file_name()))
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
