use super::{column_ident, parsed_value, primary_key, supports_range};
use crate::CodegenError;
use crate::backend::{access, find_entity, module_name, parse_ident};
use appstruct_ir::{AppIr, EntityIr, FieldTypeIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(super) fn filter_rules(
    ir: &AppIr,
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<Vec<TokenStream>, CodegenError> {
    let mut rules = Vec::new();
    for relation_field in relation_fields(entity) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        let target = find_entity(ir, &target.0)?;
        let target_module = parse_ident(&module_name(target))?;
        let source_column = column_ident(relation_field)?;
        let target_primary = column_ident(primary_key(target)?)?;
        let target_scope = access::related_scope(target, &target_module, &target.access.list)?;
        for target_field in filterable_fields(target) {
            let target_column = column_ident(target_field)?;
            let exact_key = LitStr::new(
                &format!(
                    "filter[{}.{}]",
                    relation_field.api_name, target_field.rust_name
                ),
                Span::call_site(),
            );
            let value = parsed_value(target_field, &quote! { raw });
            rules.push(quote! {
                if let Some(raw) = query.filters.get(#exact_key) {
                    use crate::entities::#target_module;
                    let value = #value;
                    let mut relation_select = #target_module::Entity::find()
                        .select_only()
                        .column(#target_module::Column::#target_primary);
                    #target_scope
                    relation_select = relation_select
                        .filter(#target_module::Column::#target_column.eq(value));
                    select = select.filter(
                        #module::Column::#source_column.in_subquery(relation_select.into_query())
                    );
                }
            });
            rules.extend(range_rules(&RangeRuleTokens {
                relation_field,
                target_field,
                module,
                target_module: &target_module,
                source_column: &source_column,
                target_primary: &target_primary,
                target_column: &target_column,
                target_scope: &target_scope,
            })?);
        }
    }
    Ok(rules)
}

struct RangeRuleTokens<'a> {
    relation_field: &'a appstruct_ir::FieldIr,
    target_field: &'a appstruct_ir::FieldIr,
    module: &'a syn::Ident,
    target_module: &'a syn::Ident,
    source_column: &'a syn::Ident,
    target_primary: &'a syn::Ident,
    target_column: &'a syn::Ident,
    target_scope: &'a TokenStream,
}

fn range_rules(tokens: &RangeRuleTokens<'_>) -> Result<Vec<TokenStream>, CodegenError> {
    let RangeRuleTokens {
        relation_field,
        target_field,
        module,
        target_module,
        source_column,
        target_primary,
        target_column,
        target_scope,
    } = tokens;
    if !supports_range(&target_field.ty) {
        return Ok(Vec::new());
    }
    [("gte", "gte"), ("lte", "lte")]
        .into_iter()
        .map(|(operator, method)| {
            let key = LitStr::new(
                &format!(
                    "filter[{}.{}][{operator}]",
                    relation_field.api_name, target_field.rust_name
                ),
                Span::call_site(),
            );
            let method = parse_ident(method)?;
            let value = parsed_value(target_field, &quote! { raw });
            Ok(quote! {
                if let Some(raw) = query.filters.get(#key) {
                    use crate::entities::#target_module;
                    let value = #value;
                    let mut relation_select = #target_module::Entity::find()
                        .select_only()
                        .column(#target_module::Column::#target_primary);
                    #target_scope
                    relation_select = relation_select
                        .filter(#target_module::Column::#target_column.#method(value));
                    select = select.filter(
                        #module::Column::#source_column
                            .in_subquery(relation_select.into_query())
                    );
                }
            })
        })
        .collect()
}

pub(super) fn filter_keys(ir: &AppIr, entity: &EntityIr) -> Result<Vec<LitStr>, CodegenError> {
    let mut keys = Vec::new();
    for relation_field in relation_fields(entity) {
        let FieldTypeIr::Relation { target } = &relation_field.ty else {
            continue;
        };
        let target = find_entity(ir, &target.0)?;
        for target_field in filterable_fields(target) {
            keys.push(LitStr::new(
                &format!(
                    "filter[{}.{}]",
                    relation_field.api_name, target_field.rust_name
                ),
                Span::call_site(),
            ));
            if supports_range(&target_field.ty) {
                for operator in ["gte", "lte"] {
                    keys.push(LitStr::new(
                        &format!(
                            "filter[{}.{}][{operator}]",
                            relation_field.api_name, target_field.rust_name
                        ),
                        Span::call_site(),
                    ));
                }
            }
        }
    }
    Ok(keys)
}

pub(super) fn has_filters(ir: &AppIr, entity: &EntityIr) -> Result<bool, CodegenError> {
    for field in relation_fields(entity) {
        let FieldTypeIr::Relation { target } = &field.ty else {
            continue;
        };
        if filterable_fields(find_entity(ir, &target.0)?)
            .next()
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn relation_fields(entity: &EntityIr) -> impl Iterator<Item = &appstruct_ir::FieldIr> {
    entity.fields.iter().filter(|field| {
        field.capabilities.filterable && matches!(field.ty, FieldTypeIr::Relation { .. })
    })
}

fn filterable_fields(entity: &EntityIr) -> impl Iterator<Item = &appstruct_ir::FieldIr> {
    entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
}
