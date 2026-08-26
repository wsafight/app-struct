use super::{render, rust_template};
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let content = if ir.tenant.enabled {
        rust_template(include_str!("../../templates/backend/tenant.rs"))?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/tenant.rs",
        content,
        ArtifactKind::RustSource,
    )])
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use crate::{Actor, ApiError, AppState, TenantId};
        use axum::{Router, http::HeaderMap};
        use sea_orm::DatabaseConnection;

        pub async fn resolve(
            _database: &DatabaseConnection,
            _headers: &HeaderMap,
            _actor: Option<&Actor>,
        ) -> Result<Option<TenantId>, ApiError> { Ok(None) }

        pub fn router() -> Router<AppState> { Router::new() }
    })
}
