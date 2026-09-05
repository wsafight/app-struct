use super::super::access;
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr};
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn write_event(
    resource: Option<&str>,
    primary: &syn::Ident,
    event: &str,
) -> TokenStream {
    let Some(resource) = resource else {
        return TokenStream::new();
    };
    let model = match event {
        "created" => quote! { model },
        "updated" => quote! { after },
        "deleted" => quote! { deleted },
        _ => unreachable!(),
    };
    quote! {
        crate::activity::record_system_event(
            &transaction, &context, #resource, #model.#primary.to_string(), #event,
        ).await?;
    }
}

pub(super) fn support(
    ir: &AppIr,
    entity: &EntityIr,
    module: &syn::Ident,
    policy: &syn::Ident,
    parse_id: &TokenStream,
) -> Result<TokenStream, CodegenError> {
    if ir.activity.resource_for_entity(&entity.id).is_none() {
        return Ok(TokenStream::new());
    }
    let read_scope = access::member_scope(entity, module, &entity.access.read)?;
    Ok(quote! {
        pub(crate) async fn authorize_activity_target(
            state: &AppState,
            context: &RequestContext<'_>,
            record_id: &str,
        ) -> Result<(), ApiError> {
            let id = record_id.to_owned();
            let id = #parse_id;
            #read_scope
            let model = #module::Entity::find_by_id(id)
                .filter(access_condition)
                .one(context).await?.ok_or(ApiError::NotFound)?;
            if !state.extensions.#policy().can_read(context, &model).await? {
                return Err(ApiError::NotFound);
            }
            Ok(())
        }
    })
}
