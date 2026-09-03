use super::extensions::operation_type;
use super::{access, parse_ident, render};
use crate::CodegenError;
use appstruct_ir::AppIr;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::LitStr;

pub(super) fn source(ir: &AppIr) -> Result<String, CodegenError> {
    if ir.commands.is_empty() && ir.queries.is_empty() {
        return render(quote! {
            use crate::AppState;
            use axum::Router;
            pub fn router() -> Router<AppState> { Router::new() }
        });
    }
    let mut routes = Vec::new();
    let mut handlers = Vec::new();
    for command in &ir.commands {
        let function = parse_ident(&snake_name(&command.rust_name))?;
        let path = LitStr::new(
            &format!("/api/commands/{}", kebab_name(&command.rust_name)),
            Span::call_site(),
        );
        let trait_name = format_ident!("{}Handler", command.rust_name);
        let input = operation_type(ir, &command.input)?;
        let output = operation_type(ir, &command.output)?;
        let allowed = access::operation_allowed(&command.access);
        routes.push(quote! { .route(#path, axum::routing::post(#function)) });
        handlers.push(quote! {
            async fn #function(
                axum::extract::State(state): axum::extract::State<AppState>,
                headers: axum::http::HeaderMap,
                axum::Json(input): axum::Json<#input>,
            ) -> Result<axum::Json<#output>, ApiError> {
                let context = state.mutation_context(&headers).await?;
                if !(#allowed) { return Err(access_denied(&context)); }
                let output = #trait_name::execute(
                    state.extensions.handlers(), &context, input
                ).await?;
                Ok(axum::Json(output))
            }
        });
    }
    for query in &ir.queries {
        let function = parse_ident(&snake_name(&query.rust_name))?;
        let path = LitStr::new(
            &format!("/api/queries/{}", kebab_name(&query.rust_name)),
            Span::call_site(),
        );
        let trait_name = format_ident!("{}Handler", query.rust_name);
        let output = operation_type(ir, &query.output)?;
        let allowed = access::operation_allowed(&query.access);
        if let Some(input) = &query.input {
            let input = operation_type(ir, input)?;
            routes.push(quote! { .route(#path, axum::routing::post(#function)) });
            handlers.push(quote! {
                async fn #function(
                    axum::extract::State(state): axum::extract::State<AppState>,
                    headers: axum::http::HeaderMap,
                    axum::Json(input): axum::Json<#input>,
                ) -> Result<axum::Json<#output>, ApiError> {
                    let context = state.mutation_context(&headers).await?;
                    if !(#allowed) { return Err(access_denied(&context)); }
                    let output = #trait_name::execute(
                        state.extensions.handlers(), &context, input
                    ).await?;
                    Ok(axum::Json(output))
                }
            });
        } else {
            routes.push(quote! { .route(#path, axum::routing::get(#function)) });
            handlers.push(quote! {
                async fn #function(
                    axum::extract::State(state): axum::extract::State<AppState>,
                    headers: axum::http::HeaderMap,
                ) -> Result<axum::Json<#output>, ApiError> {
                    let context = state.context(&headers).await?;
                    if !(#allowed) { return Err(access_denied(&context)); }
                    let output = #trait_name::execute(
                        state.extensions.handlers(), &context
                    ).await?;
                    Ok(axum::Json(output))
                }
            });
        }
    }
    render(quote! {
        use crate::{AppState, ApiError, entities, extensions::*};
        use axum::Router;

        pub fn router() -> Router<AppState> { Router::new() #(#routes)* }
        #(#handlers)*

        fn access_denied(context: &RequestContext) -> ApiError {
            if context.actor().is_some() { ApiError::Forbidden } else { ApiError::Unauthorized }
        }
    })
}

fn snake_name(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
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

fn kebab_name(value: &str) -> String {
    snake_name(value).replace('_', "-")
}
