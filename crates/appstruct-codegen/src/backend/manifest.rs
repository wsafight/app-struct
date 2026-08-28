use appstruct_ir::{AppIr, FileProviderIr, MailProviderIr};

pub(super) fn cargo(ir: &AppIr) -> String {
    let mut manifest = concat!(
        "[package]\n",
        "name = \"appstruct-generated-backend\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n\n",
        "[dependencies]\n",
        "appstruct-runtime = { path = \"runtime\" }\n",
        "async-trait = \"=0.1.89\"\n",
        "axum = \"=0.8.9\"\n",
        "base64 = \"=0.22.1\"\n",
        "chrono = { version = \"=0.4.45\", features = [\"serde\"] }\n",
        "rust_decimal = { version = \"=1.42.1\", features = [\"serde-with-str\"] }\n",
        "sea-orm = { version = \"=2.0.2\", default-features = false, features = [\"macros\", \"runtime-tokio-rustls\", \"sqlx-postgres\", \"with-chrono\", \"with-json\", \"with-rust_decimal\", \"with-uuid\"] }\n",
        "serde = { version = \"=1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"=1.0.151\"\n",
        "tokio = { version = \"=1.53.1\", features = [\"macros\", \"net\", \"rt-multi-thread\", \"signal\", \"sync\", \"time\"] }\n",
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
        manifest.push_str(concat!("argon2 = \"=0.5.3\"\n", "rand = \"=0.9.2\"\n",));
    }
    if ir.auth.oauth_enabled {
        manifest.push_str("reqwest = { version = \"=0.13.4\", default-features = false, features = [\"json\", \"form\", \"rustls\"] }\n");
    }
    if ir.auth.enabled || ir.file.enabled {
        manifest.push_str("sha2 = \"=0.10.9\"\n");
    }
    if ir.file.enabled {
        manifest.push_str("infer = \"=0.19.0\"\n");
        match ir.file.provider {
            FileProviderIr::Local => manifest.push_str("object_store = \"=0.14.1\"\n"),
            FileProviderIr::S3 => manifest
                .push_str("object_store = { version = \"=0.14.1\", features = [\"aws\"] }\n"),
        }
    }
    if ir.mail.enabled {
        manifest.push_str("minijinja = \"=2.12.0\"\n");
    }
    if ir.mail.enabled && ir.mail.provider == MailProviderIr::Resend && !ir.auth.oauth_enabled {
        manifest.push_str(
            "reqwest = { version = \"=0.13.4\", default-features = false, features = [\"json\", \"rustls\"] }\n",
        );
    }
    manifest
}

pub(super) fn runtime_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-runtime\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n\n",
        "[dependencies]\n",
        "appstruct-contracts = { path = \"../contracts\" }\n",
        "async-trait = \"=0.1.89\"\n",
        "serde = { version = \"=1.0.229\", features = [\"derive\"] }\n",
        "tokio = { version = \"=1.53.1\", features = [\"rt\", \"time\"] }\n",
        "uuid = { version = \"=1.25.0\", features = [\"serde\", \"v7\"] }\n",
    )
}

pub(super) fn contracts_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-contracts\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n",
    )
}

pub(super) fn server_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-generated-server\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n\n",
        "[dependencies]\n",
        "appstruct-app-backend = { path = \"../../app/backend\" }\n",
        "appstruct-generated-backend = { path = \"../backend\" }\n",
        "sea-orm = { version = \"=2.0.2\", default-features = false, features = [\"runtime-tokio-rustls\", \"sqlx-postgres\"] }\n",
        "tokio = { version = \"=1.53.1\", features = [\"macros\", \"net\", \"rt-multi-thread\"] }\n",
        "tracing = \"=0.1.44\"\n",
        "tracing-subscriber = { version = \"=0.3.22\", features = [\"env-filter\", \"fmt\"] }\n",
    )
}
