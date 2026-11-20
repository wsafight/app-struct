#[test]
fn production_web_template_compresses_and_caches_generated_assets() {
    let nginx = include_str!("../../templates/common/deploy/nginx.conf");
    assert!(nginx.contains("gzip on;"));
    assert!(nginx.contains("location /static/"));
    assert!(nginx.contains("max-age=31536000, immutable"));
    let api = nginx
        .split("location /api/ {")
        .nth(1)
        .and_then(|value| value.split("\n    }").next())
        .expect("API location");
    assert!(!api.contains("proxy_buffering off"));
    let realtime = nginx
        .split("location = /api/realtime/events {")
        .nth(1)
        .and_then(|value| value.split("\n    }").next())
        .expect("realtime location");
    assert!(realtime.contains("proxy_buffering off"));
}

#[test]
fn project_template_ignores_environment_secrets_but_keeps_the_example() {
    let gitignore = include_str!("../../templates/common/gitignore");
    assert!(gitignore.lines().any(|line| line == ".env"));
    assert!(gitignore.lines().any(|line| line == ".env.*"));
    assert!(gitignore.lines().any(|line| line == "!.env.example"));
}

#[test]
fn production_backend_template_persists_compilation_cache() {
    let dockerfile = include_str!("../../templates/common/Dockerfile");
    assert!(dockerfile.contains("--mount=type=cache,target=/build/target"));
    assert!(dockerfile.contains("CARGO_TARGET_DIR=/build/target"));
    assert!(dockerfile.contains("cp /build/target/release/appstruct-generated-server"));
    assert!(dockerfile.contains("COPY --from=backend-build /build/appstruct"));
}

#[test]
fn templates_emit_agent_guide_and_track_it_in_delivery_files() {
    let guide = include_str!("../../templates/common/AGENTS.md");
    assert!(guide.contains("appstruct check --deny-warnings"));
    assert!(guide.contains("generated/"));
    for template in ["minimal", "dashboard", "saas"] {
        let delivery = super::delivery_files(template).expect("supported template");
        assert!(
            delivery
                .iter()
                .any(|(path, content)| *path == "AGENTS.md" && *content == guide),
            "{template} missing AGENTS.md in delivery_files"
        );
    }
}
