use super::super::parse_ident;
use crate::CodegenError;
use appstruct_ir::{AccessRuleIr, EntityIr, FieldIr};
use proc_macro2::TokenStream;
use quote::quote;

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
    let create_guards = write_guards(entity, false)?;
    let update_guards = write_guards(entity, true)?;
    Ok(quote! {
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
