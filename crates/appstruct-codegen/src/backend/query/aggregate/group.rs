use super::super::relation;
use super::{groupable, relation_group_alias};
use crate::CodegenError;
use crate::backend::query::helpers::column_ident;
use crate::backend::{access, find_entity, module_name, parse_ident};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(super) fn arms(
    ir: &AppIr,
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<Vec<TokenStream>, CodegenError> {
    let mut arms = entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable && groupable(&field.ty))
        .map(|field| {
            let name = LitStr::new(&field.rust_name, Span::call_site());
            let column = column_ident(field)?;
            let alias = LitStr::new(&format!("group_{}", field.rust_name), Span::call_site());
            let selected = group_expression(field, module, &column);
            Ok(quote! {
                #name => {
                    if !field_read_allowed(&context, #name) {
                        return Err(access_denied(&context));
                    }
                    let expression: sea_query::SimpleExpr = #selected;
                    select = select.column_as(expression, #alias)
                        .group_by(sea_query::Expr::cust((selected_metrics.len() + selected_groups.len()).to_string()));
                }
            })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    for relation_field in relation_group_fields(entity) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        let target = find_entity(ir, &target.0)?;
        let target_module = parse_ident(&module_name(target))?;
        let source_column = column_ident(relation_field)?;
        let target_primary = column_ident(super::super::primary_key(target)?)?;
        let target_scope = access::related_join_scope(target, &target_module, &target.access.list)?;
        let source_allowed = relation::read_allowed(relation_field);
        for target_field in target
            .fields
            .iter()
            .filter(|field| field.capabilities.filterable && groupable(&field.ty))
        {
            let target_allowed = relation::read_allowed(target_field);
            let name = LitStr::new(
                &format!("{}.{}", relation_field.api_name, target_field.rust_name),
                Span::call_site(),
            );
            let target_column = column_ident(target_field)?;
            let alias = LitStr::new(
                &relation_group_alias(relation_field, target_field),
                Span::call_site(),
            );
            let selected = if matches!(target_field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal)
            {
                quote! {
                    sea_query::Expr::col((sea_query::Alias::new(#target_module::TABLE), #target_module::Column::#target_column))
                        .cast_as("text")
                }
            } else {
                quote! { #target_module::Column::#target_column }
            };
            arms.push(quote! {
                #name => {
                    if relation_group_selected {
                        return Err(ApiError::InvalidQuery(
                            "at most one relation group field is allowed".to_owned()
                        ));
                    }
                    if !(#source_allowed) || !(#target_allowed) {
                        return Err(access_denied(&context));
                    }
                    relation_group_selected = true;
                    use crate::entities::#target_module;
                    select = select.join(
                        #target_module::TABLE,
                        sea_query::Expr::col((sea_query::Alias::new(#module::TABLE), #module::Column::#source_column))
                            .equals((sea_query::Alias::new(#target_module::TABLE), #target_module::Column::#target_primary)),
                    );
                    #target_scope
                    select = select
                        .column_as(#selected, #alias)
                        .group_by(#target_module::Column::#target_column);
                }
            });
        }
    }
    Ok(arms)
}

fn group_expression(field: &FieldIr, module: &syn::Ident, column: &syn::Ident) -> TokenStream {
    if matches!(field.ty, FieldTypeIr::Date | FieldTypeIr::Datetime) {
        let template = if matches!(field.ty, FieldTypeIr::Datetime) {
            "date_trunc($1, $2 AT TIME ZONE 'UTC')::date"
        } else {
            "date_trunc($1, $2::timestamp)::date"
        };
        quote! {
            if let Some(unit) = time_bucket {
                sea_query::Expr::cust_with_exprs(#template, [
                    sea_query::Expr::val(unit).into(),
                    sea_query::Expr::col((sea_query::Alias::new(#module::TABLE), #module::Column::#column)).into(),
                ])
            } else {
                sea_query::Expr::col((sea_query::Alias::new(#module::TABLE), #module::Column::#column)).into()
            }
        }
    } else if matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal) {
        quote! { sea_query::Expr::col((sea_query::Alias::new(#module::TABLE), #module::Column::#column)).cast_as("text") }
    } else {
        quote! { sea_query::SimpleExpr::from(#module::Column::#column) }
    }
}

pub(super) fn order_arms(ir: &AppIr, entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    let mut arms = entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable && groupable(&field.ty))
        .map(|field| group_order_arm(&field.rust_name, &format!("group_{}", field.rust_name)))
        .collect::<Vec<_>>();
    for relation_field in relation_group_fields(entity) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        let target = find_entity(ir, &target.0)?;
        for target_field in target
            .fields
            .iter()
            .filter(|field| field.capabilities.filterable && groupable(&field.ty))
        {
            arms.push(group_order_arm(
                &format!("{}.{}", relation_field.api_name, target_field.rust_name),
                &relation_group_alias(relation_field, target_field),
            ));
        }
    }
    Ok(arms)
}

pub(super) fn has_relation_groups(ir: &AppIr, entity: &EntityIr) -> Result<bool, CodegenError> {
    for relation_field in relation_group_fields(entity) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        if find_entity(ir, &target.0)?
            .fields
            .iter()
            .any(|field| field.capabilities.filterable && groupable(&field.ty))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn group_order_arm(name: &str, alias: &str) -> TokenStream {
    let name = LitStr::new(name, Span::call_site());
    let alias = LitStr::new(alias, Span::call_site());
    quote! {
        #name => {
            let column = sea_query::Expr::col(sea_query::Alias::new(#alias));
            select = if order_by == "group" && descending {
                select.order_by_desc(column)
            } else {
                select.order_by_asc(column)
            };
        }
    }
}

fn relation_group_fields(entity: &EntityIr) -> impl Iterator<Item = &FieldIr> {
    entity.fields.iter().filter(|field| {
        field.capabilities.filterable && matches!(field.ty, FieldTypeIr::Relation { .. })
    })
}
