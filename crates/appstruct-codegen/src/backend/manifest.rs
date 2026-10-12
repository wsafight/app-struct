use appstruct_ir::{AppIr, FileProviderIr, MailProviderIr};

pub(super) fn cargo(ir: &AppIr) -> String {
    let mut manifest = concat!(
        "[package]\n",
        "name = \"appstruct-generated-backend\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.99\"\n\n",
        "[features]\n",
        "default = []\n",
        "test-support = []\n\n",
        "[dependencies]\n",
        "appstruct-runtime = { path = \"runtime\" }\n",
        "async-trait = \"=0.1.92\"\n",
        "axum = \"=0.8.9\"\n",
        "base64 = \"=0.23.1\"\n",
        "chrono = { version = \"=0.4.45\", features = [\"serde\"] }\n",
        "rust_decimal = { version = \"=1.43.0\", features = [\"serde-with-str\"] }\n",
        "sqlx = { version = \"=0.9.0\", default-features = false, features = [\"runtime-tokio\", \"tls-rustls-ring-webpki\", \"postgres\", \"macros\", \"chrono\", \"json\", \"rust_decimal\", \"uuid\"] }\n",
        "sea-query = { version = \"=1.0.2\", default-features = false, features = [\"derive\", \"thread-safe\", \"backend-postgres\", \"with-chrono\", \"with-json\", \"with-rust_decimal\", \"with-uuid\"] }\n",
        "sift-query = { version = \"=0.1.0\", features = [\"postgres-search\"] }\n",
        "rand = \"=0.10.3\"\n",
        "async-stream = \"=0.3.6\"\n",
        "futures-core = \"=0.3.34\"\n",
        "futures-util = \"=0.3.34\"\n",
        "serde = { version = \"=1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"=1.0.153\"\n",
        "tokio = { version = \"=1.53.2\", features = [\"io-util\", \"macros\", \"net\", \"rt-multi-thread\", \"signal\", \"sync\", \"time\"] }\n",
        "tower-http = { version = \"=0.7.1\", features = [\"cors\", \"request-id\", \"set-header\", \"trace\"] }\n",
        "tracing = \"=0.1.44\"\n",
        "tracing-subscriber = { version = \"=0.3.23\", features = [\"env-filter\", \"fmt\", \"json\"] }\n",
        "uuid = { version = \"=1.28.0\", features = [\"serde\", \"v7\"] }\n",
    )
    .to_owned();
    if ir.auth.enabled || ir.mail.enabled {
        manifest.push_str(
            "lettre = { version = \"=0.11.23\", default-features = false, features = [\"builder\", \"smtp-transport\", \"tokio1-rustls-tls\"] }\n",
        );
    }
    if ir.auth.enabled {
        manifest.push_str("argon2 = \"=0.6.0\"\n");
    }
    if ir.jobs.enabled {
        manifest.push_str("cron = \"=0.17.0\"\n");
    }
    if ir.auth.oauth_enabled
        || ir.billing.enabled
        || (ir.mail.enabled && ir.mail.provider == MailProviderIr::Resend)
    {
        manifest.push_str("reqwest = { version = \"=0.13.5\", default-features = false, features = [\"json\", \"form\", \"rustls\"] }\n");
    }
    if ir.webhooks.enabled || ir.billing.enabled {
        manifest.push_str("hmac = \"=0.13.0\"\n");
        manifest.push_str("sha2 = \"=0.11.0\"\n");
    }
    if ir.file.enabled {
        manifest.push_str("infer = \"=0.22.0\"\n");
        match ir.file.provider {
            FileProviderIr::Local => manifest.push_str("object_store = \"=0.14.2\"\n"),
            FileProviderIr::S3 => manifest
                .push_str("object_store = { version = \"=0.14.2\", features = [\"aws\"] }\n"),
        }
    }
    if ir.mail.enabled {
        manifest
            .push_str("minijinja = { version = \"=3.0.0\", features = [\"fuel\", \"serde\"] }\n");
    }
    if ir.report.enabled {
        if !ir.mail.enabled {
            manifest.push_str(
                "minijinja = { version = \"=3.0.0\", features = [\"fuel\", \"serde\"] }\n",
            );
        }
        manifest.push_str("jsonschema = \"=0.58.6\"\n");
        manifest.push_str("ring = \"=0.17.14\"\n");
        if ir.report.renderer == appstruct_ir::ReportRendererIr::Chromium {
            manifest.push_str("lopdf = { version = \"=0.45.0\", default-features = false }\n");
        }
    }
    let needs_webhook_http = ir.webhooks.enabled
        && !(ir.auth.oauth_enabled
            || ir.billing.enabled
            || (ir.mail.enabled && ir.mail.provider == MailProviderIr::Resend));
    if needs_webhook_http {
        manifest.push_str("reqwest = { version = \"=0.13.5\", default-features = false, features = [\"rustls\"] }\n");
    }
    manifest
}

pub(super) fn runtime_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-runtime\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.99\"\n\n",
        "[dependencies]\n",
        "appstruct-core = { path = \"../core\" }\n",
        "appstruct-contracts = { path = \"../contracts\" }\n",
        "async-trait = \"=0.1.92\"\n",
        "base64 = \"=0.23.1\"\n",
        "serde = { version = \"=1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"=1.0.153\"\n",
        "tokio = { version = \"=1.53.2\", features = [\"rt\", \"time\"] }\n",
        "url = \"=2.5.8\"\n",
        "uuid = { version = \"=1.28.0\", features = [\"serde\", \"v7\"] }\n",
    )
}

pub(super) fn core_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-core\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.99\"\n\n",
        "[dependencies]\n",
        "base64 = \"=0.23.1\"\n",
        "hex = \"=0.4.3\"\n",
        "serde = { version = \"=1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"=1.0.153\"\n",
        "sha2 = \"=0.11.0\"\n",
    )
}

pub(super) fn contracts_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-contracts\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.99\"\n",
    )
}

pub(super) fn server_cargo() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-generated-server\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.99\"\n\n",
        "[dependencies]\n",
        "appstruct-app-backend = { path = \"../../app/backend\" }\n",
        "appstruct-generated-backend = { path = \"../backend\" }\n",
        "tokio = { version = \"=1.53.2\", features = [\"macros\", \"net\", \"rt-multi-thread\"] }\n",
        "tracing = \"=0.1.44\"\n",
        "\n[profile.release]\n",
        "strip = \"symbols\"\n",
    )
}
