use super::super::super::{access, module_name, parse_ident};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr, FieldTypeIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub(super) fn support(ir: &AppIr, entity: &EntityIr) -> Result<TokenStream, CodegenError> {
    let module = parse_ident(&module_name(entity))?;
    let mut checks = Vec::new();
    for field in &entity.fields {
        let FieldTypeIr::Relation { target } = &field.ty else {
            continue;
        };
        let target = ir
            .entities
            .iter()
            .find(|entity| entity.id == *target)
            .expect("validated target");
        let target_module = parse_ident(&module_name(target))?;
        let policy = format_ident!("{}_policy", module_name(target));
        let field_name = parse_ident(&field.rust_name)?;
        let scope = access::member_scope(target, &target_module, &target.access.read)?;
        let id = if field.nullable {
            quote! { model.#field_name }
        } else {
            quote! { Some(model.#field_name) }
        };
        checks.push(quote! {
            if let Some(id) = #id {
                use crate::entities::#target_module;
                #scope
                let target = #target_module::Entity::find_by_id(id).filter(access_condition)
                    .lock_shared().one(context).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(context, &target).await? { return Err(ApiError::NotFound); }
            }
        });
    }
    Ok(quote! {
        async fn validate_relations(state: &AppState, context: &RequestContext<'_>, model: &#module::Model) -> Result<(), ApiError> {
            #(#checks)*
            Ok(())
        }
    })
}
