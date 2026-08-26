use super::extensions::operation_type;
use super::{parse_ident, render};
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
        routes.push(quote! { .route(#path, axum::routing::post(#function)) });
        handlers.push(quote! {
            async fn #function(
                axum::extract::State(state): axum::extract::State<AppState>,
                axum::Json(input): axum::Json<#input>,
            ) -> Result<axum::Json<#output>, ApiError> {
                let context = state.context();
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
        if let Some(input) = &query.input {
            let input = operation_type(ir, input)?;
            routes.push(quote! { .route(#path, axum::routing::post(#function)) });
            handlers.push(quote! {
                async fn #function(
                    axum::extract::State(state): axum::extract::State<AppState>,
                    axum::Json(input): axum::Json<#input>,
                ) -> Result<axum::Json<#output>, ApiError> {
                    let context = state.context();
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
                ) -> Result<axum::Json<#output>, ApiError> {
                    let context = state.context();
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
