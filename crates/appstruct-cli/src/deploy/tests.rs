use super::*;

#[test]
fn release_manifest_detects_changed_and_unsafe_artifacts() {
    let project = tempfile::tempdir().unwrap();
    for (path, content) in [
        ("appstruct.lock", "lock"),
        ("generated/.appstruct-manifest.json", "{}"),
        ("generated/web/dist/index.html", "web"),
        ("build/backend", "binary"),
        ("migrations/0001.sql", "select 1"),
    ] {
        let path = project.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    write_manifest(project.path(), Path::new("build/backend")).unwrap();
    let manifest = verify_manifest(project.path()).unwrap();
    assert_eq!(manifest.files.len(), 5);
    assert_eq!(manifest.files[0].path, "appstruct.lock");

    fs::write(
        project.path().join("generated/web/dist/index.html"),
        "changed",
    )
    .unwrap();
    assert!(
        verify_manifest(project.path())
            .unwrap_err()
            .to_string()
            .contains("does not match")
    );
    assert!(safe_relative("../outside").is_err());
    assert!(safe_relative("/absolute").is_err());
}

#[test]
fn release_manifest_rejects_unlisted_and_missing_required_artifacts() {
    let project = tempfile::tempdir().unwrap();
    for (path, content) in [
        ("appstruct.lock", "project_layout_version = 2\n"),
        ("generated/.appstruct-manifest.json", "{}"),
        ("generated/web/dist/index.html", "web"),
        (
            ".appstruct/cache/backend-target/release/appstruct-generated-backend",
            "binary",
        ),
    ] {
        let path = project.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    write_manifest(
        project.path(),
        Path::new(".appstruct/cache/backend-target/release/appstruct-generated-backend"),
    )
    .unwrap();

    fs::write(
        project.path().join("generated/web/dist/injected.js"),
        "unexpected",
    )
    .unwrap();
    assert!(
        verify_manifest(project.path())
            .unwrap_err()
            .to_string()
            .contains("is not listed")
    );

    fs::remove_file(project.path().join("generated/web/dist/injected.js")).unwrap();
    let manifest_path = project.path().join(MANIFEST_PATH);
    let mut manifest: ReleaseManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest.files.retain(|file| file.path != "appstruct.lock");
    fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        verify_manifest(project.path())
            .unwrap_err()
            .to_string()
            .contains("missing required artifact `appstruct.lock`")
    );
}

#[test]
fn deploy_config_renders_fly_and_railway_without_secrets_and_supports_check() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("spec")).unwrap();
    fs::copy(
        fixture.join("appstruct.yaml"),
        temp.path().join("appstruct.yaml"),
    )
    .unwrap();
    fs::copy(
        fixture.join("spec/project.yaml"),
        temp.path().join("spec/project.yaml"),
    )
    .unwrap();
    fs::copy(
        fixture.join("spec/identity.yaml"),
        temp.path().join("spec/identity.yaml"),
    )
    .unwrap();

    assert_eq!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: None,
                check: false,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
    let fly = fs::read_to_string(temp.path().join("deploy/fly.toml")).unwrap();
    assert!(
        fly.contains("release_command = \"/usr/local/bin/appstruct --project /app migrate apply\"")
    );
    assert!(fly.contains("path = \"/health/ready\""));
    assert_eq!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: None,
                check: true,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
    assert_ne!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: None,
                check: false,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Railway,
                output: None,
                check: false,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
    let railway = fs::read_to_string(temp.path().join("deploy/railway.json")).unwrap();
    assert!(railway.contains("\"healthcheckPath\": \"/health/ready\""));
    assert!(railway.contains("\"migrate\""));
}

#[test]
fn deploy_config_validates_flags_paths_and_force_overwrite() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("spec")).unwrap();
    for rel in ["appstruct.yaml", "spec/project.yaml", "spec/identity.yaml"] {
        fs::copy(fixture.join(rel), temp.path().join(rel)).unwrap();
    }

    assert_ne!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: None,
                check: true,
                force: true,
            },
        ),
        ExitCode::SUCCESS
    );
    for bad in ["../escape.toml", "/tmp/absolute.toml"] {
        assert_ne!(
            run(
                temp.path(),
                &DeployCommand::Config {
                    platform: DeployPlatform::Fly,
                    output: Some(PathBuf::from(bad)),
                    check: false,
                    force: false,
                },
            ),
            ExitCode::SUCCESS
        );
    }
    #[cfg(unix)]
    {
        fs::create_dir_all(temp.path().join("real")).unwrap();
        std::os::unix::fs::symlink("real", temp.path().join("linked")).unwrap();
        assert_ne!(
            run(
                temp.path(),
                &DeployCommand::Config {
                    platform: DeployPlatform::Fly,
                    output: Some(PathBuf::from("linked/fly.toml")),
                    check: false,
                    force: false,
                },
            ),
            ExitCode::SUCCESS
        );
    }

    let custom = PathBuf::from("ops/fly.toml");
    assert_eq!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: Some(custom.clone()),
                check: false,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
    fs::write(temp.path().join(&custom), "# drifted\n").unwrap();
    assert_ne!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: Some(custom.clone()),
                check: true,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: Some(custom.clone()),
                check: false,
                force: true,
            },
        ),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run(
            temp.path(),
            &DeployCommand::Config {
                platform: DeployPlatform::Fly,
                output: Some(custom),
                check: true,
                force: false,
            },
        ),
        ExitCode::SUCCESS
    );
}

#[test]
fn deploy_config_renders_auth_and_billing_provider_secret_hints() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
    let mut ir = appstruct_compiler::compile_project(&fixture).unwrap();
    ir.billing.enabled = true;
    ir.billing.provider = Some("stripe".to_owned());
    let stripe = render_platform_config(DeployPlatform::Fly, &ir);
    assert!(stripe.contains("APPSTRUCT_FRONTEND_URL"));
    assert!(stripe.contains("APPSTRUCT_STRIPE_SECRET_KEY"));
    assert!(!stripe.contains("APPSTRUCT_POLAR_ACCESS_TOKEN"));

    ir.billing.provider = Some("polar".to_owned());
    let polar = render_platform_config(DeployPlatform::Fly, &ir);
    assert!(polar.contains("APPSTRUCT_POLAR_ACCESS_TOKEN"));
    assert!(!polar.contains("APPSTRUCT_STRIPE_SECRET_KEY"));
}
