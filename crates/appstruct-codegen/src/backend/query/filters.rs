use super::helpers::column_ident;
use crate::CodegenError;
use crate::backend::{parse_ident, rust_type};
use appstruct_ir::{EntityIr, FieldIr, FieldTypeIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(super) fn filter_validation(keys: &[LitStr]) -> TokenStream {
    if keys.is_empty() {
        quote! {
            if let Some(key) = query.filters.keys().next() {
                return Err(ApiError::InvalidQuery(format!("unsupported query parameter `{key}`")));
            }
        }
    } else {
        quote! {
            for key in query.filters.keys() {
                match key.as_str() {
                    #(#keys => {},)*
                    _ => return Err(ApiError::InvalidQuery(format!("unsupported query parameter `{key}`"))),
                }
            }
        }
    }
}

pub(super) fn filter_rules(
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<Vec<TokenStream>, CodegenError> {
    let mut rules = Vec::new();
    for field in entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
    {
        let column = column_ident(field)?;
        let field_name = field.rust_name.as_str();
        let exact_key = LitStr::new(&format!("filter[{}]", field.rust_name), Span::call_site());
        let value = parsed_value(field, &quote! { raw });
        rules.push(quote! {
            if let Some(raw) = query.filters.get(#exact_key) {
                if !field_read_allowed(&context, #field_name) {
                    return Err(access_denied(&context));
                }
                let value = #value;
                select = select.filter(#module::Column::#column.eq(value));
            }
        });
        if supports_range(&field.ty) {
            for (operator, method) in [("gte", "gte"), ("lte", "lte")] {
                let key = LitStr::new(
                    &format!("filter[{}][{operator}]", field.rust_name),
                    Span::call_site(),
                );
                let method = parse_ident(method)?;
                let value = parsed_value(field, &quote! { raw });
                rules.push(quote! {
                    if let Some(raw) = query.filters.get(#key) {
                        if !field_read_allowed(&context, #field_name) {
                            return Err(access_denied(&context));
                        }
                        let value = #value;
                        select = select.filter(#module::Column::#column.#method(value));
                    }
                });
            }
        }
    }
    Ok(rules)
}

pub(super) fn filter_keys(entity: &EntityIr) -> Vec<LitStr> {
    let mut keys = Vec::new();
    for field in entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
    {
        keys.push(LitStr::new(
            &format!("filter[{}]", field.rust_name),
            Span::call_site(),
        ));
        if supports_range(&field.ty) {
            keys.push(LitStr::new(
                &format!("filter[{}][gte]", field.rust_name),
                Span::call_site(),
            ));
            keys.push(LitStr::new(
                &format!("filter[{}][lte]", field.rust_name),
                Span::call_site(),
            ));
        }
    }
    keys
}

pub(super) fn search_rule(
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<TokenStream, CodegenError> {
    let searchable = entity
        .fields
        .iter()
        .filter(|field| field.capabilities.searchable)
        .map(|field| -> Result<_, crate::CodegenError> {
            Ok((column_ident(field)?, field.rust_name.as_str()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if searchable.is_empty() {
        return Ok(quote! {
            if query.q.as_ref().is_some_and(|value| !value.trim().is_empty()) {
                return Err(ApiError::InvalidQuery("this resource does not support search".to_owned()));
            }
        });
    }
    let columns = searchable.iter().map(|(column, _)| column);
    let names = searchable.iter().map(|(_, name)| name);
    Ok(quote! {
        if let Some(term) = query.q.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            let mut condition = Condition::any();
            let mut searchable_field_allowed = false;
            #(if field_read_allowed(&context, #names) {
                searchable_field_allowed = true;
                condition = condition.add(#module::Column::#columns.contains(term));
            })*
            if !searchable_field_allowed {
                return Err(access_denied(&context));
            }
            select = select.filter(condition);
        }
    })
}

pub(super) fn sort_rules(
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter(|field| field.primary_key || field.capabilities.sortable)
        .map(|field| {
            let name = LitStr::new(&field.rust_name, Span::call_site());
            let column = column_ident(field)?;
            let primary = field.primary_key;
            Ok(quote! {
                #name => {
                    if !field_read_allowed(&context, #name) {
                        return Err(access_denied(&context));
                    }
                    select = if descending {
                        select.order_by_desc(#module::Column::#column)
                    } else {
                        select.order_by_asc(#module::Column::#column)
                    };
                    primary_sorted |= #primary;
                }
            })
        })
        .collect()
}

pub(super) fn parsed_value(field: &FieldIr, raw: &TokenStream) -> TokenStream {
    let message = LitStr::new(
        &format!("invalid value for `{}`", field.rust_name),
        Span::call_site(),
    );
    let ty = rust_type(&field.ty);
    match field.ty {
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. } => {
            quote! { #raw.to_owned() }
        }
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => quote! {
            uuid::Uuid::parse_str(#raw).map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        FieldTypeIr::Date => quote! {
            chrono::NaiveDate::parse_from_str(#raw, "%Y-%m-%d")
                .map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        FieldTypeIr::Datetime => quote! {
            #raw.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        FieldTypeIr::Json => quote! {
            serde_json::from_str::<serde_json::Value>(#raw)
                .map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        _ => quote! {
            #raw.parse::<#ty>().map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
    }
}

pub(super) fn supports_range(field_type: &FieldTypeIr) -> bool {
    matches!(
        field_type,
        FieldTypeIr::Integer
            | FieldTypeIr::Bigint
            | FieldTypeIr::Decimal
            | FieldTypeIr::Date
            | FieldTypeIr::Datetime
    )
}

pub(super) fn primary_key(entity: &EntityIr) -> Result<&FieldIr, CodegenError> {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new(format!("entity `{}` has no primary key", entity.id)))
}
