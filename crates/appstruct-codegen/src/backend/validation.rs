use super::parse_ident;
use crate::CodegenError;
use appstruct_ir::{EntityIr, FieldIr, FieldTypeIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(super) fn validation_rules(
    entity: &EntityIr,
    update: bool,
) -> Result<Vec<TokenStream>, CodegenError> {
    let mut rules = Vec::new();
    for field in entity.fields.iter().filter(|field| {
        field.generated.is_none()
            && !entity.is_workflow_field(field)
            && (!update || !field.primary_key)
    }) {
        rules.extend(text_validation(field, update)?);
        rules.extend(enum_validation(field, update)?);
        rules.extend(numeric_validation(field, update)?);
    }
    Ok(rules)
}

fn text_validation(field: &FieldIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    if !matches!(field.ty, FieldTypeIr::String | FieldTypeIr::Text) {
        return Ok(Vec::new());
    }
    let name = parse_ident(&field.rust_name)?;
    let field_name = LitStr::new(&field.api_name, Span::call_site());
    let value = input_value(field, update, &name);
    let mut rules = Vec::new();
    if let Some(limit) = field.validation.min_length {
        let message = LitStr::new(
            &format!("must contain at least {limit} characters"),
            Span::call_site(),
        );
        rules.push(quote! {
            if let Some(value) = #value && value.chars().count() < #limit as usize {
                violations.push(FieldViolation {
                    field: #field_name.to_owned(), message: #message.to_owned()
                });
            }
        });
    }
    if let Some(limit) = field.validation.max_length {
        let message = LitStr::new(
            &format!("must contain at most {limit} characters"),
            Span::call_site(),
        );
        rules.push(quote! {
            if let Some(value) = #value && value.chars().count() > #limit as usize {
                violations.push(FieldViolation {
                    field: #field_name.to_owned(), message: #message.to_owned()
                });
            }
        });
    }
    Ok(rules)
}

fn enum_validation(field: &FieldIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    let FieldTypeIr::Enum { values } = &field.ty else {
        return Ok(Vec::new());
    };
    let name = parse_ident(&field.rust_name)?;
    let field_name = LitStr::new(&field.api_name, Span::call_site());
    let value = input_value(field, update, &name);
    let allowed = values
        .iter()
        .map(|value| LitStr::new(value, Span::call_site()))
        .collect::<Vec<_>>();
    Ok(vec![quote! {
        if let Some(value) = #value && ![#(#allowed),*].contains(&value.as_str()) {
            violations.push(FieldViolation {
                field: #field_name.to_owned(),
                message: "must be one of the configured enum values".to_owned(),
            });
        }
    }])
}

fn numeric_validation(field: &FieldIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    if !matches!(
        field.ty,
        FieldTypeIr::Integer | FieldTypeIr::Bigint | FieldTypeIr::Decimal
    ) {
        return Ok(Vec::new());
    }
    let name = parse_ident(&field.rust_name)?;
    let field_name = LitStr::new(&field.api_name, Span::call_site());
    let value = input_value(field, update, &name);
    let mut rules = Vec::new();
    if let Some(minimum) = &field.validation.minimum {
        let bound = numeric_bound(field, minimum);
        let message = LitStr::new(&format!("must be at least {minimum}"), Span::call_site());
        rules.push(quote! {
            if let Some(value) = #value && value < &#bound {
                violations.push(FieldViolation {
                    field: #field_name.to_owned(), message: #message.to_owned()
                });
            }
        });
    }
    if let Some(maximum) = &field.validation.maximum {
        let bound = numeric_bound(field, maximum);
        let message = LitStr::new(&format!("must be at most {maximum}"), Span::call_site());
        rules.push(quote! {
            if let Some(value) = #value && value > &#bound {
                violations.push(FieldViolation {
                    field: #field_name.to_owned(), message: #message.to_owned()
                });
            }
        });
    }
    Ok(rules)
}

fn input_value(field: &FieldIr, update: bool, name: &syn::Ident) -> TokenStream {
    if update && field.nullable {
        quote! { input.#name.as_ref().and_then(Option::as_ref) }
    } else if update || field.nullable || field.default.is_some() {
        quote! { input.#name.as_ref() }
    } else {
        quote! { Some(&input.#name) }
    }
}

fn numeric_bound(field: &FieldIr, value: &str) -> TokenStream {
    match field.ty {
        FieldTypeIr::Integer => {
            let value = value.parse::<i32>().expect("compiler validated bound");
            quote! { #value }
        }
        FieldTypeIr::Bigint => {
            let value = value.parse::<i64>().expect("compiler validated bound");
            quote! { #value }
        }
        FieldTypeIr::Decimal => {
            quote! { rust_decimal::Decimal::from_str_exact(#value).expect("validated bound") }
        }
        _ => unreachable!(),
    }
}
