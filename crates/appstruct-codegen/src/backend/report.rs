use super::render;
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

mod assets;
mod contract;
mod crypto;
mod lifecycle;
mod renderer;
mod routes;
mod worker;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.report.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    let mut artifacts = vec![Artifact::text(
        "backend/src/report.rs",
        source,
        ArtifactKind::RustSource,
    )];
    if ir.report.enabled && ir.report.renderer == appstruct_ir::ReportRendererIr::Chromium {
        artifacts.extend(assets::files());
        artifacts.push(Artifact::text(
            "backend/src/report/adapter.rs",
            include_str!("../../templates/backend/report_adapter.rs"),
            ArtifactKind::RustSource,
        ));
    }
    Ok(artifacts)
}

fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let contract = contract::source(ir);
    let crypto = crypto::source();
    let renderer =
        (ir.report.renderer == appstruct_ir::ReportRendererIr::Capture).then(renderer::source);
    let routes = routes::source(ir);
    let worker = worker::source(ir.report.renderer);
    render(quote! {
        use crate::{ApiError, AppState, RequestContext};
        use axum::{
            Json, Router,
            body::Body,
            extract::{Path, Query, State},
            http::{HeaderMap, HeaderValue, StatusCode, header},
            response::Response,
            routing::{get, post},
        };
        use base64::Engine as _;
        use ring::{aead, rand::{SecureRandom, SystemRandom}};
        use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
        use serde::{Deserialize, Serialize};
        use sha2::{Digest, Sha256};

        #contract
        #crypto
        #renderer
        #routes
        #worker
    })
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use crate::AppState;
        use axum::Router;
        pub fn router() -> Router<AppState> { Router::new() }
    })
}
