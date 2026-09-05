mod activity;
mod aggregates;
mod bulk;
mod fields;
mod lookup;
mod realtime;
mod workflow;
mod write;
use super::query::list_support;
use super::validation::validation_rules;
use super::{module_name, parse_ident, render, rust_type};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

#[allow(clippy::too_many_lines)]
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
    let hooks = format_ident!("{}_hooks", module_name(entity));
    let policy = format_ident!("{}_policy", module_name(entity));
    let activity_resource = ir
        .activity
        .resource_for_entity(&entity.id)
        .map(|resource| resource.resource.as_str());
    let list = list_support(ir, entity, &module, &policy)?;
    let lookup = lookup::support(
        entity,
        &module,
        &policy,
        &super::query::helpers::column_ident(primary_key(entity)?)?,
        &parse_id,
    )?;
    let aggregate = super::query::aggregate::aggregate_support(ir, entity, &module, &policy)?;
    let collections = aggregates::support(ir, entity)?;
    let collection_routes = collections.routes;
    let collection_support = collections.tokens;
    let collection_guard = collections.guard;
    let handlers = write::handlers(
        entity,
        &write::HandlerContext {
            module: &module,
            hooks: &hooks,
            policy: &policy,
            parse_id: &parse_id,
            primary: &primary,
            soft_delete: entity.views.soft_delete,
            activity_resource,
        },
        &create_values,
        active_default.as_ref(),
        &updates,
    )?;
    let validators = validation_functions(entity)?;
    let field_access = fields::support(entity, &module)?;
    let realtime = if ir.realtime.enabled {
        realtime::support(entity, &module, &policy, &parse_id)?
    } else {
        TokenStream::new()
    };
    let activity = activity::support(ir, entity, &module, &policy, &parse_id)?;
    let workflow = workflow::support(
        ir,
        entity,
        &workflow::WorkflowContext {
            module: &module,
            hooks: &hooks,
            policy: &policy,
            parse_id: &parse_id,
            primary: &primary,
        },
    )?;
    let workflow_routes = workflow.routes;
    let workflow_support = workflow.tokens;
    let list_scope = super::access::scope(entity, &module, &entity.access.list)?;
    let create_allowed = super::access::create_allowed(entity, &entity.access.create)?;
    let update_allowed = super::access::update_allowed(entity, &entity.access.update)?;
    let delete_allowed = super::access::row_allowed(entity, &entity.access.delete)?;
    let bulk = bulk::source(
        entity,
        &bulk::SourceContext {
            module: &module,
            primary: &primary,
            hooks: &hooks,
            policy: &policy,
            list_scope: &list_scope,
            create_allowed: &create_allowed,
            delete_allowed: &delete_allowed,
            update_allowed: &update_allowed,
            create_values: &create_values,
            active_default: active_default.as_ref(),
            updates: &updates,
            activity_resource,
        },
    )?;
    let restore_route = entity
        .views
        .soft_delete
        .then(|| quote! { .route("/_restore", axum::routing::post(restore)).route("/_trash", get(trash)) });
    let model_trait = (!entity.views.soft_delete).then(|| quote! { ModelTrait, });
    render(quote! {
        use crate::{AppState, ApiError, FieldViolation, RequestContext, entities::#module};
        use axum::{
            Json, Router,
            extract::{Path, State},
            http::{HeaderMap, StatusCode, header},
            routing::get,
        };
        use sea_orm::{
            ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel,
            #model_trait ColumnTrait as _, QuerySelect, TransactionTrait, TryIntoModel,
        };
        use serde::{Deserialize, Serialize};
        use std::collections::BTreeMap;

        #[derive(Clone, Debug, Default, Deserialize)]
        pub struct CreateInput { #(#create_fields,)* }
        #[derive(Clone, Debug, Default, Deserialize)]
        pub struct UpdateInput { #(#update_fields,)* }
        pub fn router() -> Router<AppState> {
            Router::new()
                .route("/", get(list).post(create))
                .route("/_aggregate", get(aggregate))
                .route("/_lookup", get(lookup))
                .route("/_bulk", axum::routing::patch(bulk_update).delete(bulk_delete))
                .route("/_export.csv", get(export_csv))
                .route("/_import.csv", axum::routing::post(import_csv))
                #restore_route
                #workflow_routes
                #collection_routes
                .route("/{id}", get(read).patch(update).delete(delete))
                #collection_guard
        }
        #list
        #lookup
        #aggregate
        #collection_support
        #handlers
        #validators
        #field_access
        #realtime
        #activity
        #workflow_support
        #bulk
    })
}
fn validation_functions(entity: &EntityIr) -> Result<TokenStream, CodegenError> {
    let create_rules = validation_rules(entity, false)?;
    let update_rules = validation_rules(entity, true)?;
    Ok(quote! {
        #[allow(unused_mut, unused_variables)]
        fn validate_create(input: &CreateInput) -> Result<(), ApiError> {
            let mut violations = Vec::new();
            #(#create_rules)*
            finish_validation(violations)
        }

        #[allow(unused_mut, unused_variables)]
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
    if update || field.default.is_some() || field.write_access.is_some() {
        ty = quote! { Option<#ty> };
    }
    let depth = u8::from(field.nullable)
        + u8::from(update || field.default.is_some() || field.write_access.is_some());
    let scalar = super::scalar::attributes(&field.ty, depth);
    Ok(quote! { #scalar pub #name: #ty })
}

fn create_values(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter_map(|field| create_value(entity, field).transpose())
        .collect()
}

fn create_value(entity: &EntityIr, field: &FieldIr) -> Result<Option<TokenStream>, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let value = if entity.is_workflow_field(field) {
        let initial = &entity.workflow.as_ref().expect("workflow exists").initial;
        quote! { #initial.to_owned() }
    } else {
        match field.generated {
            Some(GeneratedValueIr::UuidV7) => quote! { uuid::Uuid::now_v7() },
            Some(GeneratedValueIr::Now) if matches!(field.ty, FieldTypeIr::Date) => {
                quote! { chrono::Utc::now().date_naive() }
            }
            Some(GeneratedValueIr::Now) => quote! { chrono::Utc::now() },
            Some(GeneratedValueIr::AutoIncrement) => return Ok(None),
            Some(GeneratedValueIr::Revision) => quote! { 1_i64 },
            Some(GeneratedValueIr::Tenant) => quote! { context.require_tenant()? },
            None => field.default.as_ref().map_or_else(
                || {
                    if field.write_access.is_some() && !field.nullable {
                        let field_name = field.api_name.clone();
                        let message = format!("field `{}` is required", field.api_name);
                        quote! {
                            input.#name.ok_or_else(|| ApiError::Validation(vec![FieldViolation {
                                field: #field_name.to_owned(), message: #message.to_owned()
                            }]))?
                        }
                    } else {
                        quote! { input.#name }
                    }
                },
                |default| {
                    let default = default_expression(field, default);
                    quote! { input.#name.unwrap_or_else(|| #default) }
                },
            ),
        }
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
    entity.fields.iter().filter(move |field| {
        field.generated.is_none()
            && !entity.is_workflow_field(field)
            && (!update || !field.primary_key)
    })
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
