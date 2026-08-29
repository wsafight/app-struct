use super::{find_entity, render};
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    if !ir.auth.enabled {
        return Ok(vec![Artifact::text(
            "backend/src/auth.rs",
            disabled_source()?,
            ArtifactKind::RustSource,
        )]);
    }
    Ok(vec![
        generated(
            "backend/src/auth/config.rs",
            config_source(ir).map_err(|error| {
                CodegenError::new(format!("auth config generation failed: {error}"))
            })?,
        ),
        generated("backend/src/auth/mod.rs", template("auth/mod.rs")?),
        generated("backend/src/auth/session.rs", template("auth/session.rs")?),
        generated(
            "backend/src/auth/handlers.rs",
            handlers_template(ir.auth.oauth_enabled)?,
        ),
        generated("backend/src/auth/mail.rs", template("auth/mail.rs")?),
    ])
}

fn generated(path: &str, content: String) -> Artifact {
    Artifact::text(path, content, ArtifactKind::RustSource)
}

fn config_source(ir: &AppIr) -> Result<String, CodegenError> {
    let user_id = ir
        .auth
        .user_entity
        .as_ref()
        .expect("compiler requires auth user entity");
    let user = find_entity(ir, &user_id.0)?;
    let id = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler requires auth user primary key");
    let email = user
        .fields
        .iter()
        .find(|field| field.api_name == "email")
        .expect("compiler requires auth user email");
    let registration = ir.auth.registration_enabled;
    let password_reset = ir.auth.password_reset_enabled;
    let oauth = ir.auth.oauth_enabled;
    let jobs = ir.jobs.enabled;
    let mail = ir.mail.enabled;
    let file = ir.file.enabled;
    let tenant = ir.tenant.enabled;
    let audit = ir.audit.enabled;
    let default_role = ir
        .auth
        .default_role
        .as_deref()
        .expect("compiler requires default role");
    let user_table = &user.table_name;
    let user_id_column = &id.column_name;
    let user_email_column = &email.column_name;
    render(quote! {
        pub const REGISTRATION_ENABLED: bool = #registration;
        pub const PASSWORD_RESET_ENABLED: bool = #password_reset;
        #[allow(dead_code)]
        pub const OAUTH_ENABLED: bool = #oauth;
        pub const JOBS_ENABLED: bool = #jobs;
        pub const MAIL_ENABLED: bool = #mail;
        pub const FILE_ENABLED: bool = #file;
        pub const TENANT_ENABLED: bool = #tenant;
        pub const AUDIT_ENABLED: bool = #audit;
        pub const DEFAULT_ROLE: &str = #default_role;
        pub const USER_TABLE: &str = #user_table;
        pub const USER_ID_COLUMN: &str = #user_id_column;
        pub const USER_EMAIL_COLUMN: &str = #user_email_column;
    })
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use crate::{Actor, ApiError, AppState};
        use axum::{Router, http::HeaderMap};
        use sea_orm::DatabaseConnection;
        use tower_http::cors::CorsLayer;

        #[derive(Clone, Default)]
        pub struct AuthState;

        impl AuthState {
            pub fn from_env() -> Result<Self, String> { Ok(Self) }
            pub async fn actor(
                &self,
                _database: &DatabaseConnection,
                _headers: &HeaderMap,
            ) -> Result<Option<Actor>, ApiError> { Ok(None) }
            pub async fn verify_csrf(
                &self,
                _database: &DatabaseConnection,
                _headers: &HeaderMap,
            ) -> Result<(), ApiError> { Ok(()) }
            pub fn cors_layer(&self) -> CorsLayer { CorsLayer::permissive() }
        }

        pub fn router() -> Router<AppState> { Router::new() }
    })
}

fn template(name: &str) -> Result<String, CodegenError> {
    let source = match name {
        "auth/mod.rs" => include_str!("../../templates/backend/auth/mod.rs"),
        "auth/session.rs" => include_str!("../../templates/backend/auth/session.rs"),
        "auth/handlers.rs" => include_str!("../../templates/backend/auth/handlers.rs"),
        "auth/mail.rs" => include_str!("../../templates/backend/auth/mail.rs"),
        _ => unreachable!(),
    };
    super::rust_template(source)
}

fn handlers_template(oauth_enabled: bool) -> Result<String, CodegenError> {
    let source = include_str!("../../templates/backend/auth/handlers.rs");
    if oauth_enabled {
        return super::rust_template(source);
    }
    let source = strip_oauth_sections(source)?;
    super::rust_template(&source)
}

fn strip_oauth_sections(source: &str) -> Result<String, CodegenError> {
    const START: &str = "// appstruct:oauth:start";
    const END: &str = "// appstruct:oauth:end";
    let mut output = String::with_capacity(source.len());
    let mut remaining = source;
    while let Some(start) = remaining.find(START) {
        output.push_str(&remaining[..start]);
        let after_start = &remaining[start + START.len()..];
        let end = after_start.find(END).ok_or_else(|| {
            CodegenError::new("auth OAuth template section is missing its end marker")
        })?;
        remaining = &after_start[end + END.len()..];
    }
    output.push_str(remaining);
    Ok(output)
}
