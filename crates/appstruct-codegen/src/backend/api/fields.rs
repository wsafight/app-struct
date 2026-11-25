use super::super::parse_ident;
use crate::CodegenError;
use appstruct_ir::{AccessRuleIr, EntityIr, FieldIr};
use proc_macro2::TokenStream;
use quote::quote;

#[allow(clippy::too_many_lines)]
pub(super) fn support(entity: &EntityIr, module: &syn::Ident) -> Result<TokenStream, CodegenError> {
    let read_arms = field_access_arms(entity, |field| field.read_access.as_ref());
    let write_arms = field_access_arms(entity, |field| field.write_access.as_ref());
    let read_redactions = entity
        .fields
        .iter()
        .filter(|field| field.read_access.is_some())
        .map(|field| {
            let key = field.rust_name.as_str();
            quote! {
                if !field_read_allowed(context, #key) {
                    if let serde_json::Value::Object(object) = &mut value {
                        object.remove(#key);
                    }
                }
            }
        });
    let mask_ops = entity
        .fields
        .iter()
        .filter_map(|field| field.mask.as_ref().map(|mask| (field, mask)))
        .map(|(field, mask)| {
            let key = field.rust_name.as_str();
            let exempt = mask.except_roles.iter().map(String::as_str);
            let strategy_code = match mask.strategy {
                appstruct_ir::MaskStrategyIr::Email => quote! { mask_email(raw) },
                appstruct_ir::MaskStrategyIr::Phone => quote! { mask_phone(raw) },
                appstruct_ir::MaskStrategyIr::Last4 => quote! { mask_last4(raw) },
                appstruct_ir::MaskStrategyIr::Redact => quote! { "[REDACTED]".to_owned() },
            };
            quote! {
                let exempt_roles: &[&str] = &[#(#exempt),*];
                let is_exempt = !exempt_roles.is_empty()
                    && roles.iter().any(|role| exempt_roles.contains(&role.as_str()));
                if !is_exempt {
                    if let serde_json::Value::Object(object) = value {
                        if let Some(serde_json::Value::String(raw)) = object.get(#key) {
                            let masked = #strategy_code;
                            object.insert(#key.to_owned(), serde_json::Value::String(masked));
                        }
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    let has_masks = !mask_ops.is_empty();
    let mask_helpers = if has_masks {
        quote! {
            #[allow(dead_code)]
            fn mask_email(value: &str) -> String {
                match value.split_once('@') {
                    Some((local, domain)) => {
                        let first = local.chars().next().map_or_else(String::new, |c| c.to_string());
                        format!("{first}***@{domain}")
                    }
                    None => "[REDACTED]".to_owned(),
                }
            }

            #[allow(dead_code)]
            fn mask_phone(value: &str) -> String {
                let digits: Vec<char> = value.chars().filter(char::is_ascii_digit).collect();
                if digits.len() >= 4 {
                    let tail: String = digits[digits.len() - 4..].iter().collect();
                    format!("***-***-{tail}")
                } else {
                    "[REDACTED]".to_owned()
                }
            }

            #[allow(dead_code)]
            fn mask_last4(value: &str) -> String {
                let chars: Vec<char> = value.chars().collect();
                if chars.len() > 4 {
                    let tail: String = chars[chars.len() - 4..].iter().collect();
                    format!("****{tail}")
                } else {
                    "****".to_owned()
                }
            }
        }
    } else {
        quote! {}
    };
    let create_guards = write_guards(entity, false)?;
    let update_guards = write_guards(entity, true)?;
    Ok(quote! {
        #mask_helpers

        #[allow(dead_code, unused_variables, unused_mut)]
        fn apply_field_masks_for_roles(roles: &[String], value: &mut serde_json::Value) {
            #(#mask_ops)*
        }

        #[allow(dead_code, unused_variables, unused_mut)]
        fn apply_field_masks(context: &RequestContext, value: &mut serde_json::Value) {
            let roles = context.actor().map_or(&[][..], |actor| actor.roles.as_slice());
            apply_field_masks_for_roles(roles, value);
        }

        #[allow(dead_code, unused_variables, unused_mut)]
        fn field_read_allowed(context: &RequestContext, field: &str) -> bool {
            match field {
                #(#read_arms,)*
                _ => true,
            }
        }

        #[allow(dead_code, unused_variables, unused_mut)]
        fn field_write_allowed(context: &RequestContext, field: &str) -> bool {
            match field {
                #(#write_arms,)*
                _ => true,
            }
        }

        #[allow(dead_code, unused_variables, unused_mut)]
        fn redact_model(
            context: &RequestContext,
            model: #module::Model,
        ) -> Result<serde_json::Value, ApiError> {
            let mut value = serde_json::to_value(model).map_err(|_| ApiError::Internal)?;
            #(#read_redactions)*
            apply_field_masks(context, &mut value);
            Ok(value)
        }

        #[allow(dead_code, unused_variables, unused_mut)]
        fn authorize_create_fields(
            context: &RequestContext,
            input: &CreateInput,
        ) -> Result<(), ApiError> {
            #(#create_guards)*
            Ok(())
        }

        #[allow(dead_code, unused_variables, unused_mut)]
        fn authorize_update_fields(
            context: &RequestContext,
            input: &UpdateInput,
        ) -> Result<(), ApiError> {
            #(#update_guards)*
            Ok(())
        }
    })
}

fn field_access_arms(
    entity: &EntityIr,
    access: impl Fn(&FieldIr) -> Option<&AccessRuleIr>,
) -> Vec<TokenStream> {
    entity
        .fields
        .iter()
        .filter_map(|field| {
            access(field).map(|rule| {
                let name = &field.rust_name;
                let allowed = super::super::access::operation_allowed(rule);
                quote! { #name => #allowed }
            })
        })
        .collect()
}

fn write_guards(entity: &EntityIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter(|field| {
            field.write_access.is_some()
                && field.generated.is_none()
                && !entity.is_workflow_field(field)
                && (!update || !field.primary_key)
        })
        .map(|field| {
            let name = parse_ident(&field.rust_name)?;
            let key = field.rust_name.as_str();
            let check = quote! {
                if !field_write_allowed(context, #key) {
                    return Err(access_denied(context));
                }
            };
            if update || field.nullable || field.default.is_some() {
                Ok(quote! { if input.#name.is_some() { #check } })
            } else {
                Ok(check)
            }
        })
        .collect()
}
