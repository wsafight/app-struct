mod write;

use super::query::list_support;
use super::validation::validation_rules;
use super::{module_name, parse_ident, render, rust_type};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub(super) fn source(ir: &AppIr, entity: &EntityIr) -> Result<String, CodegenError> {
    let module = parse_ident(&module_name(entity))?;
    let create_fields = dto_fields(entity, false)?;
    let update_fields = dto_fields(entity, true)?;
    let create_values = create_values(entity)?;
    let active_default = entity
        .fields
        .iter()
        .any(|field| matches!(field.generated, Some(GeneratedValueIr::AutoIncrement)))
        .then(|| quote! { ..Default::default() });
    let updates = update_values(entity)?;
    let parse_id = parse_id_expression(primary_key(entity)?);
    let primary = parse_ident(&primary_key(entity)?.rust_name)?;
    let list = list_support(ir, entity, &module)?;
    let aggregate = super::query::aggregate_support(ir, entity, &module)?;
    let hooks = format_ident!("{}_hooks", module_name(entity));
    let policy = format_ident!("{}_policy", module_name(entity));
    let handlers = write::handlers(
        entity,
        &write::HandlerContext {
            module: &module,
            hooks: &hooks,
            policy: &policy,
            parse_id: &parse_id,
            primary: &primary,
        },
        &create_values,
        active_default.as_ref(),
        &updates,
    )?;
    let validators = validation_functions(entity)?;

    render(quote! {
        use crate::{AppState, ApiError, FieldViolation, RequestContext, entities::#module};
        use axum::{
            Json, Router,
            extract::{Path, State},
            http::{HeaderMap, StatusCode, header},
            routing::get,
        };
        use sea_orm::{
            ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel, ModelTrait,
            QuerySelect, TransactionTrait, TryIntoModel,
        };
        use serde::{Deserialize, Serialize};
        use std::collections::BTreeMap;

        #[derive(Debug, Deserialize)]
        pub struct CreateInput { #(#create_fields,)* }

        #[derive(Debug, Deserialize)]
        pub struct UpdateInput { #(#update_fields,)* }

        pub fn router() -> Router<AppState> {
            Router::new()
                .route("/", get(list).post(create))
                .route("/_aggregate", get(aggregate))
                .route("/{id}", get(read).patch(update).delete(delete))
        }

        #list
        #aggregate
        #handlers
        #validators
    })
}

fn validation_functions(entity: &EntityIr) -> Result<TokenStream, CodegenError> {
    let create_rules = validation_rules(entity, false)?;
    let update_rules = validation_rules(entity, true)?;
    Ok(quote! {
        fn validate_create(input: &CreateInput) -> Result<(), ApiError> {
            let mut violations = Vec::new();
            #(#create_rules)*
            finish_validation(violations)
        }

        fn validate_update(input: &UpdateInput) -> Result<(), ApiError> {
            let mut violations = Vec::new();
            #(#update_rules)*
            finish_validation(violations)
        }

        fn finish_validation(violations: Vec<FieldViolation>) -> Result<(), ApiError> {
            if violations.is_empty() { Ok(()) } else { Err(ApiError::Validation(violations)) }
        }

        fn access_denied(context: &RequestContext) -> ApiError {
            if context.actor().is_some() { ApiError::Forbidden } else { ApiError::Unauthorized }
        }
    })
}

fn dto_fields(entity: &EntityIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    writable_fields(entity, update)
        .map(|field| dto_field(field, update))
        .collect()
}

fn dto_field(field: &FieldIr, update: bool) -> Result<TokenStream, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let mut ty = rust_type(&field.ty);
    if field.nullable {
        ty = quote! { Option<#ty> };
    }
    if update || field.default.is_some() {
        ty = quote! { Option<#ty> };
    }
    Ok(quote! { pub #name: #ty })
}

fn create_values(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter_map(|field| create_value(field).transpose())
        .collect()
}

fn create_value(field: &FieldIr) -> Result<Option<TokenStream>, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let value = match field.generated {
        Some(GeneratedValueIr::UuidV7) => quote! { uuid::Uuid::now_v7() },
        Some(GeneratedValueIr::Now) if matches!(field.ty, FieldTypeIr::Date) => {
            quote! { chrono::Utc::now().date_naive() }
        }
        Some(GeneratedValueIr::Now) => quote! { chrono::Utc::now() },
        Some(GeneratedValueIr::AutoIncrement) => return Ok(None),
        Some(GeneratedValueIr::Revision) => quote! { 1_i64 },
        Some(GeneratedValueIr::Tenant) => quote! { context.require_tenant()? },
        None => field.default.as_ref().map_or_else(
            || quote! { input.#name },
            |default| {
                let default = default_expression(field, default);
                quote! { input.#name.unwrap_or_else(|| #default) }
            },
        ),
    };
    Ok(Some(quote! { #name: Set(#value) }))
}

fn default_expression(field: &FieldIr, default: &str) -> TokenStream {
    match field.ty {
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. } => {
            quote! { #default.to_owned() }
        }
        FieldTypeIr::Integer => {
            let value = default
                .parse::<i32>()
                .expect("compiler validated integer default");
            quote! { #value }
        }
        FieldTypeIr::Bigint => {
            let value = default
                .parse::<i64>()
                .expect("compiler validated bigint default");
            quote! { #value }
        }
        FieldTypeIr::Boolean => {
            let value = default
                .parse::<bool>()
                .expect("compiler validated boolean default");
            quote! { #value }
        }
        FieldTypeIr::Decimal => {
            quote! { rust_decimal::Decimal::from_str_exact(#default).expect("validated default") }
        }
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => {
            quote! { uuid::Uuid::parse_str(#default).expect("validated default") }
        }
        FieldTypeIr::Date => {
            quote! { chrono::NaiveDate::parse_from_str(#default, "%Y-%m-%d").expect("validated default") }
        }
        FieldTypeIr::Datetime => {
            quote! { #default.parse::<chrono::DateTime<chrono::Utc>>().expect("validated default") }
        }
        FieldTypeIr::Json => {
            quote! { serde_json::from_str(#default).expect("validated default") }
        }
    }
}

fn update_values(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    writable_fields(entity, true)
        .map(|field| {
            let name = parse_ident(&field.rust_name)?;
            let value = if copy_type(&field.ty) {
                quote! { *value }
            } else {
                quote! { value.clone() }
            };
            Ok(quote! {
                if let Some(value) = &input.#name { active.#name = Set(#value); }
            })
        })
        .collect()
}

fn copy_type(field_type: &FieldTypeIr) -> bool {
    matches!(
        field_type,
        FieldTypeIr::Uuid
            | FieldTypeIr::Integer
            | FieldTypeIr::Bigint
            | FieldTypeIr::Decimal
            | FieldTypeIr::Boolean
            | FieldTypeIr::Date
            | FieldTypeIr::Datetime
            | FieldTypeIr::Relation { .. }
    )
}

fn writable_fields(entity: &EntityIr, update: bool) -> impl Iterator<Item = &FieldIr> {
    entity
        .fields
        .iter()
        .filter(move |field| field.generated.is_none() && (!update || !field.primary_key))
}

fn primary_key(entity: &EntityIr) -> Result<&FieldIr, CodegenError> {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new(format!("entity `{}` has no primary key", entity.id)))
}

fn parse_id_expression(field: &FieldIr) -> TokenStream {
    match field.ty {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => {
            quote! { uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)? }
        }
        FieldTypeIr::Integer => quote! { id.parse::<i32>().map_err(|_| ApiError::InvalidId)? },
        FieldTypeIr::Bigint => quote! { id.parse::<i64>().map_err(|_| ApiError::InvalidId)? },
        _ => quote! { id },
    }
}
