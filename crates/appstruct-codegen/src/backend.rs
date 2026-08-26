mod access;
mod api;
mod audit;
mod auth;
mod context;
mod entity;
mod extensions;
mod jobs;
mod mail;
mod manifest;
mod operations;
mod query;
mod tenant;
mod validation;

use crate::{Artifact, ArtifactKind, CodegenError, format_rust, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldTypeIr};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let mut artifacts = vec![
        Artifact::text(
            "backend/Cargo.toml",
            manifest::cargo(ir),
            ArtifactKind::RustManifest,
        ),
        Artifact::text(
            "backend/src/main.rs",
            rust_template(include_str!("../templates/backend/main.rs"))?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/error.rs",
            rust_template(include_str!("../templates/backend/error.rs"))?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/entities/mod.rs",
            module_source(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/api/mod.rs",
            module_source(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/lib.rs",
            library_source(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/openapi.rs",
            crate::openapi::rust_source(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/extensions.rs",
            extensions::source(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/operations.rs",
            operations::source(ir)?,
            ArtifactKind::RustSource,
        ),
    ];
    artifacts.extend(audit::plan(ir)?);
    artifacts.extend(auth::plan(ir)?);
    artifacts.extend(jobs::plan(ir)?);
    artifacts.extend(mail::plan(ir)?);
    artifacts.extend(tenant::plan(ir)?);
    artifacts.extend(entity_artifacts(ir)?);
    Ok(artifacts)
}

fn entity_artifacts(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    if ir.entities.is_empty() {
        return Ok(Vec::new());
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(ir.entities.len());
    let chunk_size = ir.entities.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles = ir
            .entities
            .chunks(chunk_size)
            .map(|entities| {
                scope.spawn(move || {
                    entities
                        .iter()
                        .map(|entity| plan_entity(ir, entity))
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect::<Vec<_>>();
        let mut artifacts = Vec::with_capacity(ir.entities.len() * 2);
        for handle in handles {
            let planned = handle
                .join()
                .map_err(|_| CodegenError::new("backend generation worker panicked"))??;
            artifacts.extend(planned.into_iter().flatten());
        }
        Ok(artifacts)
    })
}

fn plan_entity(ir: &AppIr, entity: &EntityIr) -> Result<[Artifact; 2], CodegenError> {
    let module = module_name(entity);
    let entity_source = entity::source(ir, entity)
        .map_err(|error| CodegenError::new(format!("entity `{module}` failed: {error}")))?;
    let api_source = api::source(entity)
        .map_err(|error| CodegenError::new(format!("API `{module}` failed: {error}")))?;
    Ok([
        Artifact::text(
            format!("backend/src/entities/{module}.rs"),
            entity_source,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            format!("backend/src/api/{module}.rs"),
            api_source,
            ArtifactKind::RustSource,
        ),
    ])
}

fn module_source(ir: &AppIr) -> Result<String, CodegenError> {
    let modules = ir
        .entities
        .iter()
        .map(|entity| format!("pub mod {};", module_name(entity)))
        .collect::<Vec<_>>()
        .join("\n");
    format_rust(&format!("{}{}\n", generated_header("//"), modules))
}

fn library_source(ir: &AppIr) -> Result<String, CodegenError> {
    let routes = ir
        .entities
        .iter()
        .map(|entity| {
            let module = parse_ident(&module_name(entity))?;
            let path = LitStr::new(&format!("/api/{}/", entity.table_name), Span::call_site());
            Ok(quote! { .nest(#path, api::#module::router()) })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let auth_exports = if ir.auth.enabled {
        quote! { pub use auth::{AuthMailSender, AuthState, DevMailSender, SmtpMailSender}; }
    } else {
        quote! { pub use auth::AuthState; }
    };
    let service_exports = service_exports(ir);
    render(quote! {
        pub mod api;
        pub mod entities;
        pub mod extensions;
        mod audit;
        mod auth;
        mod error;
        mod jobs;
        mod mail;
        mod openapi;
        mod operations;
        mod tenant;

        pub use error::{ApiError, FieldViolation};
        pub use extensions::{Actor, AppExtensions, HookOperation, RequestContext, TenantId};
        #service_exports
        #auth_exports

        use axum::{Router, extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, routing::get};
        use sea_orm::DatabaseConnection;
        use tower_http::{
            request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
            trace::TraceLayer,
        };

        #[derive(Clone)]
        pub struct AppState {
            pub database: DatabaseConnection,
            pub extensions: AppExtensions,
            pub auth: AuthState,
            pub mail: MailState,
        }

        impl AppState {
            pub async fn context(&self, headers: &HeaderMap) -> Result<RequestContext<'_>, ApiError> {
                let actor = self.auth.actor(&self.database, headers).await?;
                let tenant = tenant::resolve(&self.database, headers, actor.as_ref()).await?;
                Ok(RequestContext::connection(&self.database, &self.mail, actor, tenant))
            }
        }

        pub fn router(database: DatabaseConnection, extensions: AppExtensions) -> Router {
            let auth = AuthState::from_env().expect("invalid AppStruct auth configuration");
            let mail = MailState::from_env(database.clone())
                .expect("invalid AppStruct mail configuration");
            router_with_services(database, extensions, auth, mail)
        }

        pub fn router_with_auth(
            database: DatabaseConnection,
            extensions: AppExtensions,
            auth: AuthState,
        ) -> Router {
            let mail = MailState::from_env(database.clone())
                .expect("invalid AppStruct mail configuration");
            router_with_services(database, extensions, auth, mail)
        }

        pub fn router_with_services(
            database: DatabaseConnection,
            extensions: AppExtensions,
            auth: AuthState,
            mail: MailState,
        ) -> Router {
            let cors = auth.cors_layer();
            Router::new()
                #(#routes)*
                .merge(operations::router())
                .merge(audit::router())
                .merge(auth::router())
                .merge(tenant::router())
                .route("/health/live", get(health))
                .route("/health/ready", get(readiness))
                .route("/openapi.json", get(openapi))
                .layer(cors)
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(TraceLayer::new_for_http())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .with_state(AppState { database, extensions, auth, mail })
        }

        async fn health() -> StatusCode { StatusCode::NO_CONTENT }

        async fn readiness(State(state): State<AppState>) -> StatusCode {
            if state.database.ping().await.is_ok() {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }

        async fn openapi() -> impl IntoResponse {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], openapi::OPENAPI_JSON)
        }
    })
}

fn service_exports(ir: &AppIr) -> TokenStream {
    let mail_job_exports = (ir.jobs.enabled && ir.mail.enabled).then(|| {
        quote! { pub use jobs::{MailJobHandler, MailJobPayload}; }
    });
    quote! {
        pub use jobs::{
            Job, JobError, JobHandler, JobHandlerError, JobReceipt, JobWorker, JobWorkerHandle,
        };
        #mail_job_exports
        pub use mail::{MailDelivery, MailError, MailMessage, MailProvider, MailState};
    }
}

pub(super) fn rust_type(field_type: &FieldTypeIr) -> TokenStream {
    match field_type {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => quote! { uuid::Uuid },
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. } => quote! { String },
        FieldTypeIr::Integer => quote! { i32 },
        FieldTypeIr::Bigint => quote! { i64 },
        FieldTypeIr::Decimal => quote! { rust_decimal::Decimal },
        FieldTypeIr::Boolean => quote! { bool },
        FieldTypeIr::Date => quote! { chrono::NaiveDate },
        FieldTypeIr::Datetime => quote! { chrono::DateTime<chrono::Utc> },
        FieldTypeIr::Json => quote! { serde_json::Value },
    }
}

pub(super) fn optional_type(base: TokenStream, nullable: bool) -> TokenStream {
    if nullable {
        quote! { Option<#base> }
    } else {
        base
    }
}

pub(super) fn find_entity<'ir>(ir: &'ir AppIr, id: &str) -> Result<&'ir EntityIr, CodegenError> {
    ir.entities
        .iter()
        .find(|entity| entity.id.0 == id)
        .ok_or_else(|| CodegenError::new(format!("missing entity `{id}`")))
}

pub(super) fn module_name(entity: &EntityIr) -> String {
    let mut output = String::new();
    for (index, character) in entity.rust_name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

pub(super) fn parse_ident(value: &str) -> Result<Ident, CodegenError> {
    syn::parse_str(value)
        .map_err(|error| CodegenError::new(format!("invalid Rust identifier `{value}`: {error}")))
}

pub(super) fn render(tokens: TokenStream) -> Result<String, CodegenError> {
    let syntax = syn::parse2(tokens)
        .map_err(|error| CodegenError::new(format!("generated Rust did not parse: {error}")))?;
    format_rust(&format!(
        "{}{}",
        generated_header("//"),
        prettyplease::unparse(&syntax)
    ))
}

fn rust_template(source: &str) -> Result<String, CodegenError> {
    format_rust(&format!("{}{}", generated_header("//"), source))
}
