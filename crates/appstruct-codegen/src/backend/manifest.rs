pub(super) fn cargo(auth_enabled: bool) -> String {
    let mut manifest = concat!(
        "[package]\n",
        "name = \"appstruct-generated-backend\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n\n",
        "[dependencies]\n",
        "async-trait = \"0.1.89\"\n",
        "axum = \"0.8.9\"\n",
        "chrono = { version = \"0.4.45\", features = [\"serde\"] }\n",
        "rust_decimal = { version = \"1.42.1\", features = [\"serde-with-str\"] }\n",
        "sea-orm = { version = \"2.0.2\", default-features = false, features = [\"macros\", \"runtime-tokio-rustls\", \"sqlx-postgres\", \"with-chrono\", \"with-json\", \"with-rust_decimal\", \"with-uuid\"] }\n",
        "serde = { version = \"1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"1.0.151\"\n",
        "tokio = { version = \"1.53.1\", features = [\"macros\", \"net\", \"rt-multi-thread\"] }\n",
        "tower-http = { version = \"0.7.0\", features = [\"cors\", \"trace\"] }\n",
        "tracing = \"0.1.44\"\n",
        "tracing-subscriber = { version = \"0.3.22\", features = [\"env-filter\", \"fmt\"] }\n",
        "uuid = { version = \"1.25.0\", features = [\"serde\", \"v7\"] }\n",
    )
    .to_owned();
    if auth_enabled {
        manifest.push_str(concat!(
            "argon2 = \"0.5.3\"\n",
            "base64 = \"0.22.1\"\n",
            "lettre = { version = \"0.11.19\", default-features = false, features = [\"builder\", \"smtp-transport\", \"tokio1-rustls-tls\"] }\n",
            "rand = \"0.9.2\"\n",
            "sha2 = \"0.10.9\"\n",
        ));
    }
    manifest
}
