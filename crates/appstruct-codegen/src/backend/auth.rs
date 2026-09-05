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
        generated("backend/src/auth/admin.rs", template("auth/admin.rs")?),
        generated(
            "backend/src/auth/admin_schedules.rs",
            template("auth/admin_schedules.rs")?,
        ),
        generated(
            "backend/src/auth/admin_storage.rs",
            template("auth/admin_storage.rs")?,
        ),
        generated("backend/src/auth/session.rs", template("auth/session.rs")?),
        generated(
            "backend/src/auth/handlers.rs",
            template("auth/handlers.rs")?,
        ),
        generated(
            "backend/src/auth/admin_webhooks.rs",
            template("auth/admin_webhooks.rs")?,
        ),
        generated("backend/src/auth/mail.rs", template("auth/mail.rs")?),
        generated(
            "backend/src/auth/oauth.rs",
            oauth_template(ir.auth.oauth_enabled)?,
        ),
        generated(
            "backend/src/auth/recovery.rs",
            template("auth/recovery.rs")?,
        ),
        generated(
            "backend/src/auth/saved_views.rs",
            template("auth/saved_views.rs")?,
        ),
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
    let webhooks = ir.webhooks.enabled;
    let mail = ir.mail.enabled;
    let file = ir.file.enabled;
    let tenant = ir.tenant.enabled;
    let audit = ir.audit.enabled;
    let resources = ir.entities.iter().map(|entity| entity.id.0.as_str());
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
        pub const WEBHOOKS_ENABLED: bool = #webhooks;
        pub const MAIL_ENABLED: bool = #mail;
        pub const FILE_ENABLED: bool = #file;
        pub const TENANT_ENABLED: bool = #tenant;
        pub const AUDIT_ENABLED: bool = #audit;
        pub const SAVED_VIEW_RESOURCES: &[&str] = &[#(#resources),*];
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
            pub async fn actor_for_mutation(
                &self,
                _database: &DatabaseConnection,
                _headers: &HeaderMap,
            ) -> Result<Option<Actor>, ApiError> { Ok(None) }
            pub async fn verify_csrf(
                &self,
                _database: &DatabaseConnection,
                _headers: &HeaderMap,
            ) -> Result<(), ApiError> { Ok(()) }
            pub fn cors_layer(&self) -> CorsLayer {
                if std::env::var("APPSTRUCT_ENV").as_deref() == Ok("production") {
                    CorsLayer::new()
                } else {
                    CorsLayer::permissive()
                }
            }
        }

        pub fn router() -> Router<AppState> { Router::new() }
    })
}

fn template(name: &str) -> Result<String, CodegenError> {
    let source = match name {
        "auth/mod.rs" => include_str!("../../templates/backend/auth/mod.rs"),
        "auth/admin.rs" => include_str!("../../templates/backend/auth/admin.rs"),
        "auth/admin_schedules.rs" => {
            include_str!("../../templates/backend/auth/admin_schedules.rs")
        }
        "auth/admin_storage.rs" => {
            include_str!("../../templates/backend/auth/admin_storage.rs")
        }
        "auth/session.rs" => include_str!("../../templates/backend/auth/session.rs"),
        "auth/handlers.rs" => include_str!("../../templates/backend/auth/handlers.rs"),
        "auth/admin_webhooks.rs" => include_str!("../../templates/backend/auth/admin_webhooks.rs"),
        "auth/mail.rs" => include_str!("../../templates/backend/auth/mail.rs"),
        "auth/recovery.rs" => include_str!("../../templates/backend/auth/recovery.rs"),
        "auth/saved_views.rs" => include_str!("../../templates/backend/auth/saved_views.rs"),
        _ => unreachable!(),
    };
    super::rust_template(source)
}

fn oauth_template(enabled: bool) -> Result<String, CodegenError> {
    let source = if enabled {
        include_str!("../../templates/backend/auth/oauth.rs")
    } else {
        include_str!("../../templates/backend/auth/oauth_disabled.rs")
    };
    super::rust_template(source)
}
