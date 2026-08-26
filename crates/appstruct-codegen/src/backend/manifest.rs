use appstruct_ir::{AppIr, MailProviderIr};

pub(super) fn cargo(ir: &AppIr) -> String {
    let mut manifest = concat!(
        "[package]\n",
        "name = \"appstruct-generated-backend\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n\n",
        "[dependencies]\n",
        "async-trait = \"=0.1.89\"\n",
        "axum = \"=0.8.9\"\n",
        "chrono = { version = \"=0.4.45\", features = [\"serde\"] }\n",
        "rust_decimal = { version = \"=1.42.1\", features = [\"serde-with-str\"] }\n",
        "sea-orm = { version = \"=2.0.2\", default-features = false, features = [\"macros\", \"runtime-tokio-rustls\", \"sqlx-postgres\", \"with-chrono\", \"with-json\", \"with-rust_decimal\", \"with-uuid\"] }\n",
        "serde = { version = \"=1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"=1.0.151\"\n",
        "tokio = { version = \"=1.53.1\", features = [\"macros\", \"net\", \"rt-multi-thread\"] }\n",
        "tower-http = { version = \"=0.7.0\", features = [\"cors\", \"request-id\", \"trace\"] }\n",
        "tracing = \"=0.1.44\"\n",
        "tracing-subscriber = { version = \"=0.3.22\", features = [\"env-filter\", \"fmt\"] }\n",
        "uuid = { version = \"=1.25.0\", features = [\"serde\", \"v7\"] }\n",
    )
    .to_owned();
    if ir.auth.enabled || ir.mail.enabled {
        manifest.push_str(
            "lettre = { version = \"=0.11.19\", default-features = false, features = [\"builder\", \"smtp-transport\", \"tokio1-rustls-tls\"] }\n",
        );
    }
    if ir.auth.enabled {
        manifest.push_str(concat!(
            "argon2 = \"=0.5.3\"\n",
            "base64 = \"=0.22.1\"\n",
            "rand = \"=0.9.2\"\n",
            "sha2 = \"=0.10.9\"\n",
        ));
    }
    if ir.mail.enabled {
        manifest.push_str("minijinja = \"=2.12.0\"\n");
    }
    if ir.mail.enabled && ir.mail.provider == MailProviderIr::Resend {
        manifest.push_str(
            "reqwest = { version = \"=0.13.4\", default-features = false, features = [\"json\", \"rustls\"] }\n",
        );
    }
    manifest
}
