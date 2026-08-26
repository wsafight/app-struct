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
    for field in entity
        .fields
        .iter()
        .filter(|field| field.generated.is_none() && (!update || !field.primary_key))
    {
        rules.extend(field_validation(field, update)?);
    }
    Ok(rules)
}

fn field_validation(field: &FieldIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    if !matches!(field.ty, FieldTypeIr::String | FieldTypeIr::Text) {
        return Ok(Vec::new());
    }
    let name = parse_ident(&field.rust_name)?;
    let field_name = LitStr::new(&field.api_name, Span::call_site());
    let value = if update && field.nullable {
        quote! { input.#name.as_ref().and_then(Option::as_ref) }
    } else if update || field.nullable {
        quote! { input.#name.as_ref() }
    } else {
        quote! { Some(&input.#name) }
    };
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
