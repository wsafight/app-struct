mod api;
mod entity;
mod extensions;
mod manifest;
mod operations;
mod query;
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
            manifest::cargo(),
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
    for entity in &ir.entities {
        let module = module_name(entity);
        artifacts.push(Artifact::text(
            format!("backend/src/entities/{module}.rs"),
            entity::source(ir, entity)?,
            ArtifactKind::RustSource,
        ));
        artifacts.push(Artifact::text(
            format!("backend/src/api/{module}.rs"),
            api::source(entity)?,
            ArtifactKind::RustSource,
        ));
    }
    Ok(artifacts)
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
    render(quote! {
        pub mod api;
        pub mod entities;
        pub mod extensions;
        mod error;
        mod openapi;
        mod operations;

        pub use error::{ApiError, FieldViolation};
        pub use extensions::{AppExtensions, HookOperation, RequestContext};

        use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
        use sea_orm::DatabaseConnection;
        use tower_http::{cors::CorsLayer, trace::TraceLayer};

        #[derive(Clone)]
        pub struct AppState {
            pub database: DatabaseConnection,
            pub extensions: AppExtensions,
        }

        impl AppState {
            pub fn context(&self) -> RequestContext {
                RequestContext::new(self.database.clone())
            }
        }

        pub fn router(database: DatabaseConnection, extensions: AppExtensions) -> Router {
            Router::new()
                #(#routes)*
                .merge(operations::router())
                .route("/health/live", get(health))
                .route("/openapi.json", get(openapi))
                .layer(CorsLayer::permissive())
                .layer(TraceLayer::new_for_http())
                .with_state(AppState { database, extensions })
        }

        async fn health() -> StatusCode { StatusCode::NO_CONTENT }

        async fn openapi() -> impl IntoResponse {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], openapi::OPENAPI_JSON)
        }
    })
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
