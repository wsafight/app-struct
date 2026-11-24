use super::extensions::operation_type;
use super::{access, parse_ident, render};
use crate::CodegenError;
use appstruct_ir::{AppIr, HttpOperationIr};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::LitStr;

#[allow(clippy::too_many_lines)]
pub(super) fn source(ir: &AppIr) -> Result<String, CodegenError> {
    if ir.commands.is_empty() && ir.queries.is_empty() {
        return render(quote! {
            use crate::AppState;
            use axum::Router;
            pub fn router() -> Router<AppState> { Router::new() }
        });
    }
    let has_http = ir.commands.iter().any(|command| command.http.is_some())
        || ir.queries.iter().any(|query| query.http.is_some());
    let mut routes = Vec::new();
    let mut handlers = Vec::new();
    for command in &ir.commands {
        let function = parse_ident(&snake_name(&command.rust_name))?;
        let path = LitStr::new(
            &format!("/api/commands/{}", kebab_name(&command.rust_name)),
            Span::call_site(),
        );
        let input = operation_type(ir, &command.input)?;
        let output = operation_type(ir, &command.output)?;
        let allowed = access::operation_allowed(&command.access);
        let dispatch = if let Some(http) = &command.http {
            http_dispatch(
                &command.rust_name,
                "command",
                http,
                &quote! { Some(&input) },
            )
        } else {
            let trait_name = format_ident!("{}Handler", command.rust_name);
            quote! { #trait_name::execute(state.extensions.handlers(), &context, input).await? }
        };
        routes.push(quote! { .route(#path, axum::routing::post(#function)) });
        handlers.push(quote! {
            async fn #function(
                axum::extract::State(state): axum::extract::State<AppState>,
                headers: axum::http::HeaderMap,
                axum::Json(input): axum::Json<#input>,
            ) -> Result<axum::Json<#output>, ApiError> {
                let context = state.mutation_context(&headers).await?;
                if !(#allowed) { return Err(access_denied(&context)); }
                let output: #output = #dispatch;
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
        let output = operation_type(ir, &query.output)?;
        let allowed = access::operation_allowed(&query.access);
        if let Some(input) = &query.input {
            let input = operation_type(ir, input)?;
            let dispatch = if let Some(http) = &query.http {
                http_dispatch(&query.rust_name, "query", http, &quote! { Some(&input) })
            } else {
                let trait_name = format_ident!("{}Handler", query.rust_name);
                quote! { #trait_name::execute(state.extensions.handlers(), &context, input).await? }
            };
            routes.push(quote! { .route(#path, axum::routing::post(#function)) });
            handlers.push(quote! {
                async fn #function(
                    axum::extract::State(state): axum::extract::State<AppState>,
                    headers: axum::http::HeaderMap,
                    axum::Json(input): axum::Json<#input>,
                ) -> Result<axum::Json<#output>, ApiError> {
                    let context = state.mutation_context(&headers).await?;
                    if !(#allowed) { return Err(access_denied(&context)); }
                    let output: #output = #dispatch;
                    Ok(axum::Json(output))
                }
            });
        } else {
            let dispatch = if let Some(http) = &query.http {
                http_dispatch(
                    &query.rust_name,
                    "query",
                    http,
                    &quote! { Option::<&serde_json::Value>::None },
                )
            } else {
                let trait_name = format_ident!("{}Handler", query.rust_name);
                quote! { #trait_name::execute(state.extensions.handlers(), &context).await? }
            };
            routes.push(quote! { .route(#path, axum::routing::get(#function)) });
            handlers.push(quote! {
                async fn #function(
                    axum::extract::State(state): axum::extract::State<AppState>,
                    headers: axum::http::HeaderMap,
                ) -> Result<axum::Json<#output>, ApiError> {
                    let context = state.context(&headers).await?;
                    if !(#allowed) { return Err(access_denied(&context)); }
                    let output: #output = #dispatch;
                    Ok(axum::Json(output))
                }
            });
        }
    }
    let http_helper = has_http.then(http_helper_tokens);
    render(quote! {
        use crate::{AppState, ApiError, entities, extensions::*};
        use axum::Router;

        pub fn router() -> Router<AppState> { Router::new() #(#routes)* }
        #(#handlers)*
        #http_helper

        fn access_denied(context: &RequestContext) -> ApiError {
            if context.actor().is_some() { ApiError::Forbidden } else { ApiError::Unauthorized }
        }
    })
}

fn http_dispatch(
    operation_name: &str,
    kind: &str,
    http: &HttpOperationIr,
    input_expr: &TokenStream,
) -> TokenStream {
    let op_lit = LitStr::new(operation_name, Span::call_site());
    let kind_lit = LitStr::new(kind, Span::call_site());
    let url_env = LitStr::new(&http.url_env, Span::call_site());
    let secret_env = if let Some(secret) = &http.secret_env {
        let lit = LitStr::new(secret, Span::call_site());
        quote! { Some(#lit) }
    } else {
        quote! { None }
    };
    let timeout_ms = http.timeout_ms;
    let retries = http.retries;
    quote! {
        invoke_http_operation(
            &context,
            #op_lit,
            #kind_lit,
            #url_env,
            #secret_env,
            #timeout_ms,
            #retries,
            #input_expr,
        ).await?
    }
}

fn http_helper_tokens() -> TokenStream {
    quote! {
        async fn invoke_http_operation<I, O>(
            context: &RequestContext,
            operation: &str,
            kind: &str,
            url_env: &str,
            secret_env: Option<&str>,
            timeout_ms: u64,
            retries: u8,
            input: Option<&I>,
        ) -> Result<O, ApiError>
        where
            I: serde::Serialize,
            O: serde::de::DeserializeOwned,
        {
            let url = std::env::var(url_env)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or(ApiError::Internal)?;
            let envelope = serde_json::json!({
                "operation": operation,
                "kind": kind,
                "tenant_id": context.tenant_id().map(|id| id.0.to_string()),
                "actor": context.actor().map(|actor| serde_json::json!({
                    "id": actor.id.to_string(),
                    "roles": actor.roles,
                })),
                "input": input,
            });
            let body = serde_json::to_vec(&envelope).map_err(|_| ApiError::Internal)?;
            let signature = secret_env
                .and_then(|name| std::env::var(name).ok())
                .filter(|secret| !secret.is_empty())
                .map(|secret| {
                    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
                    let tag = ring::hmac::sign(&key, &body);
                    let mut hex = String::with_capacity(tag.as_ref().len() * 2);
                    for byte in tag.as_ref() {
                        use std::fmt::Write as _;
                        let _ = write!(&mut hex, "{byte:02x}");
                    }
                    format!("sha256={hex}")
                });
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(timeout_ms))
                .build()
                .map_err(|_| ApiError::Internal)?;
            let max_attempts = u32::from(retries) + 1;
            for attempt in 0..max_attempts {
                let mut builder = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .header("x-appstruct-operation", operation)
                    .header("x-appstruct-kind", kind);
                if let Some(tenant_id) = context.tenant_id() {
                    builder = builder.header("x-appstruct-tenant", tenant_id.0.to_string());
                }
                if let Some(actor) = context.actor() {
                    builder = builder.header("x-appstruct-actor", actor.id.to_string());
                }
                if let Some(sig) = &signature {
                    builder = builder.header("x-appstruct-signature", sig);
                }
                match builder.body(body.clone()).send().await {
                    Ok(response) if response.status().is_success() => {
                        let bytes = response.bytes().await.map_err(|_| ApiError::Internal)?;
                        return serde_json::from_slice(&bytes).map_err(|_| ApiError::Internal);
                    }
                    Ok(response) if response.status().is_server_error() && attempt + 1 < max_attempts => {
                        tokio::time::sleep(std::time::Duration::from_millis(100u64 << attempt)).await;
                    }
                    Err(_) if attempt + 1 < max_attempts => {
                        tokio::time::sleep(std::time::Duration::from_millis(100u64 << attempt)).await;
                    }
                    _ => return Err(ApiError::Internal),
                }
            }
            Err(ApiError::Internal)
        }
    }
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
