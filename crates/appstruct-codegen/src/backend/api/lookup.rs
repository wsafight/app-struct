use crate::{CodegenError, backend::access};
use appstruct_ir::EntityIr;
use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(super) fn support(
    entity: &EntityIr,
    module: &Ident,
    policy: &Ident,
    primary: &Ident,
    parse_id: &TokenStream,
) -> Result<TokenStream, CodegenError> {
    let scope = access::scope(entity, module, &entity.access.read)?;
    Ok(quote! {
        #[derive(Deserialize)]
        struct LookupQuery { ids: String }

        async fn lookup(State(state): State<AppState>, headers: HeaderMap, axum::extract::Query(query): axum::extract::Query<LookupQuery>) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
            let context = state.context(&headers).await?;
            let raw_ids = query.ids.split(',').collect::<Vec<_>>();
            if raw_ids.is_empty() || raw_ids.len() > 100 || raw_ids.iter().any(|id| id.is_empty()) {
                return Err(ApiError::InvalidQuery("lookup requires between 1 and 100 IDs".to_owned()));
            }
            let ids = raw_ids.into_iter().map(|id| { let id = id.to_owned(); Ok(#parse_id) }).collect::<Result<Vec<_>, ApiError>>()?;
            let mut select = #module::Entity::find();
            #scope
            let models = select.filter(#module::Column::#primary.is_in(ids)).all(&state.database).await?;
            let mut records = Vec::with_capacity(models.len());
            for model in models {
                if state.extensions.#policy().can_read(&context, &model).await? {
                    records.push(redact_model(&context, model)?);
                }
            }
            Ok(Json(records))
        }
    })
}
