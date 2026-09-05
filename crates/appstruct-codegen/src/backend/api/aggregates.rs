use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr};
use proc_macro2::TokenStream;
use quote::quote;

mod input;
mod parent;
mod relations;
mod rows;

pub(super) struct Support {
    pub routes: TokenStream,
    pub tokens: TokenStream,
    pub guard: TokenStream,
}

pub(super) fn support(ir: &AppIr, entity: &EntityIr) -> Result<Support, CodegenError> {
    let mut support = Support {
        routes: TokenStream::new(),
        tokens: TokenStream::new(),
        guard: TokenStream::new(),
    };
    for aggregate in &entity.views.aggregates {
        let (routes, tokens) = parent::support(ir, entity, aggregate)?;
        support.routes.extend(routes);
        support.tokens.extend(tokens);
    }
    if let Some(aggregate) = ir
        .entities
        .iter()
        .flat_map(|parent| &parent.views.aggregates)
        .find(|aggregate| aggregate.child == entity.id)
    {
        let tokens = rows::support(ir, entity, aggregate)?;
        support.tokens.extend(quote! {
            pub(crate) mod aggregate_rows {
                use super::*;
                #tokens
            }
            async fn aggregate_read_only(
                request: axum::extract::Request,
                next: axum::middleware::Next,
            ) -> axum::response::Response {
                use axum::response::IntoResponse as _;
                if !matches!(*request.method(), axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS) {
                    return (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "GET, HEAD, OPTIONS")],
                        Json(serde_json::json!({ "error": { "code": "AGGREGATE_OWNED", "message": "Edit this collection through its parent" } }))).into_response();
                }
                next.run(request).await
            }
        });
        support.guard = quote! { .route_layer(axum::middleware::from_fn(aggregate_read_only)) };
    }
    Ok(support)
}
