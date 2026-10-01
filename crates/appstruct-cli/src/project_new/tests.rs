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
fn production_backend_template_persists_compilation_cache() {
    let dockerfile = include_str!("../../templates/common/Dockerfile");
    assert!(dockerfile.contains("--mount=type=cache,target=/build/target"));
    assert!(dockerfile.contains("CARGO_TARGET_DIR=/build/target"));
    assert!(dockerfile.contains("cp /build/target/release/appstruct-generated-server"));
    assert!(dockerfile.contains("COPY --from=backend-build /build/appstruct"));
}
