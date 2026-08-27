mod access;
mod api;
mod audit;
mod auth;
mod context;
mod entity;
mod extensions;
mod file;
mod jobs;
mod mail;
mod manifest;
mod operations;
mod query;
mod runtime;
mod startup;
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
            "backend/runtime/Cargo.toml",
            manifest::runtime_cargo(),
            ArtifactKind::RustManifest,
        ),
        Artifact::text(
            "backend/contracts/Cargo.toml",
            manifest::contracts_cargo(),
            ArtifactKind::RustManifest,
        ),
        Artifact::text(
            "backend/contracts/src/lib.rs",
            embedded_crate_source(include_str!("../../appstruct-contracts/src/lib.rs")),
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/runtime/src/lib.rs",
            embedded_crate_source(include_str!("../../appstruct-runtime/src/lib.rs")),
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/runtime/src/lifecycle.rs",
            format!(
                "{}{}",
                generated_header("//"),
                include_str!("../../appstruct-runtime/src/lifecycle.rs")
            ),
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "server/Cargo.toml",
            manifest::server_cargo(),
            ArtifactKind::RustManifest,
        ),
        Artifact::text(
            "server/src/main.rs",
            rust_template(include_str!("../templates/backend/server_main.rs"))?,
            ArtifactKind::RustSource,
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
    artifacts.extend(file::plan(ir)?);
    artifacts.extend(jobs::plan(ir)?);
    artifacts.extend(mail::plan(ir)?);
    artifacts.extend(tenant::plan(ir)?);
    artifacts.extend(entity_artifacts(ir)?);
    Ok(artifacts)
}

fn embedded_crate_source(source: &str) -> String {
    let source = source
        .lines()
        .map(|line| {
            line.strip_prefix("//!")
                .map_or(line.to_owned(), |line| format!("//{line}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}{}\n", generated_header("//"), source)
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
    let runtime = runtime::source(ir, &routes)?;
    let runtime_api_version = appstruct_runtime::RUNTIME_API_VERSION;
    render(quote! {
        pub mod api;
        pub mod entities;
        pub mod extensions;
        mod audit;
        mod auth;
        mod error;
        mod file;
        mod jobs;
        mod mail;
        mod openapi;
        mod operations;
        mod tenant;

        pub use error::{ApiError, FieldViolation};
        pub use appstruct_runtime::{
            Actor, ModuleDescriptor, ModuleEvent, ModuleObserver, ModulePhase, ModulePlan,
            ModuleRuntime, ModuleStarter, RUNTIME_API_VERSION, ServiceHandle, ServiceHandles,
            ShutdownError, ShutdownFailure, ShutdownFailureKind, ShutdownReport, StartupError,
            TenantId,
        };
        pub const GENERATED_RUNTIME_API_VERSION: u32 = #runtime_api_version;
        const _: [(); GENERATED_RUNTIME_API_VERSION as usize] =
            [(); appstruct_runtime::RUNTIME_API_VERSION as usize];
        pub use extensions::{AppExtensions, HookOperation, RequestContext};
        #service_exports
        #auth_exports
        #runtime
    })
}

fn service_exports(ir: &AppIr) -> TokenStream {
    let mail_job_exports = (ir.jobs.enabled && ir.mail.enabled).then(|| {
        quote! { pub use jobs::{MailJobHandler, MailJobPayload}; }
    });
    quote! {
        pub use file::{FileError, FileMetadata, FileProvider, FileState};
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
    Ok(format!(
        "{}{}",
        generated_header("//"),
        prettyplease::unparse(&syntax)
    ))
}

fn rust_template(source: &str) -> Result<String, CodegenError> {
    format_rust(&format!("{}{}", generated_header("//"), source))
}
