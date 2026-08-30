use super::super::access;
use crate::CodegenError;
use appstruct_ir::EntityIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn support(
    entity: &EntityIr,
    module: &syn::Ident,
    policy: &syn::Ident,
    parse_id: &TokenStream,
) -> Result<TokenStream, CodegenError> {
    let list_scope = access::scope(entity, module, &entity.access.list)?;
    let read_scope = access::member_scope(entity, module, &entity.access.read)?;
    let read_allowed = access::row_allowed(entity, &entity.access.read)?;
    let tenant_allowed = if entity.tenant_scoped {
        quote! { model.tenant_id == context.require_tenant()? }
    } else {
        quote! { true }
    };
    Ok(quote! {
        pub(crate) async fn authorize_realtime_scope(
            state: &AppState,
            context: &RequestContext<'_>,
            record_id: Option<&str>,
        ) -> Result<(), ApiError> {
            if let Some(record_id) = record_id {
                let id = record_id.to_owned();
                let id = #parse_id;
                #read_scope
                let model = #module::Entity::find_by_id(id)
                    .filter(access_condition)
                    .one(&state.database).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(context, &model).await? {
                    return Err(ApiError::NotFound);
                }
                return Ok(());
            }

            let mut select = #module::Entity::find();
            #list_scope
            let _ = select;
            if !state.extensions.#policy().can_list(context).await? {
                return Err(access_denied(context));
            }
            Ok(())
        }

        pub(crate) async fn authorize_realtime_event(
            state: &AppState,
            context: &RequestContext<'_>,
            event: &crate::RealtimeEvent,
        ) -> Result<bool, ApiError> {
            let Ok(model) = serde_json::from_value::<#module::Model>(event.data.clone()) else {
                return Ok(false);
            };
            if !(#tenant_allowed) || !(#read_allowed) {
                return Ok(false);
            }
            state.extensions.#policy().can_read(context, &model).await
        }
    })
}
