use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(super) fn handlers(
    module: &Ident,
    hooks: &Ident,
    policy: &Ident,
    parse_id: &TokenStream,
    create_values: &[TokenStream],
    active_default: Option<&TokenStream>,
    updates: &[TokenStream],
) -> TokenStream {
    let read = read_handler(module, policy, parse_id);
    let create = create_handler(module, hooks, policy, create_values, active_default);
    let update = update_handler(module, hooks, policy, parse_id, updates);
    let delete = delete_handler(module, hooks, policy, parse_id);
    let helpers = helper_functions(module, hooks);
    quote! {
        #read
        #create
        #update
        #delete
        #helpers
    }
}

fn read_handler(module: &Ident, policy: &Ident, parse_id: &TokenStream) -> TokenStream {
    quote! {
        async fn read(
            State(state): State<AppState>,
            Path(id): Path<String>,
        ) -> Result<([(header::HeaderName, String); 1], Json<#module::Model>), ApiError> {
            let context = state.context();
            let id = #parse_id;
            let model = #module::Entity::find_by_id(id)
                .one(&state.database).await?.ok_or(ApiError::NotFound)?;
            if !state.extensions.#policy().can_read(&context, &model).await? {
                return Err(ApiError::NotFound);
            }
            Ok((etag_header(&model), Json(model)))
        }
    }
}

fn create_handler(
    module: &Ident,
    hooks: &Ident,
    policy: &Ident,
    create_values: &[TokenStream],
    active_default: Option<&TokenStream>,
) -> TokenStream {
    quote! {
        async fn create(
            State(state): State<AppState>,
            Json(mut input): Json<CreateInput>,
        ) -> Result<(StatusCode, [(header::HeaderName, String); 1], Json<#module::Model>), ApiError> {
            let context = state.context();
            state.extensions.#hooks().before_validate_create(&context, &mut input).await?;
            validate_create(&input)?;
            let transaction = state.database.begin().await?;
            let model = {
                let context = RequestContext::transaction(&transaction);
                state.extensions.#hooks().before_create(&context, &mut input).await?;
                validate_create(&input)?;
                if !state.extensions.#policy().can_create(&context, &input).await? {
                    return Err(ApiError::Forbidden);
                }
                let active = #module::ActiveModel { #(#create_values,)* #active_default };
                let model = active.insert(&transaction).await?;
                state.extensions.#hooks().after_create(&context, &model).await?;
                model
            };
            transaction.commit().await?;
            run_after_commit(&state, crate::HookOperation::Create, &model).await;
            Ok((StatusCode::CREATED, etag_header(&model), Json(model)))
        }
    }
}

fn update_handler(
    module: &Ident,
    hooks: &Ident,
    policy: &Ident,
    parse_id: &TokenStream,
    updates: &[TokenStream],
) -> TokenStream {
    quote! {
        async fn update(
            State(state): State<AppState>,
            Path(id): Path<String>,
            headers: HeaderMap,
            Json(mut input): Json<UpdateInput>,
        ) -> Result<([(header::HeaderName, String); 1], Json<#module::Model>), ApiError> {
            let expected = expected_revision(&headers)?;
            let context = state.context();
            state.extensions.#hooks().before_validate_update(&context, &mut input).await?;
            validate_update(&input)?;
            let id = #parse_id;
            let transaction = state.database.begin().await?;
            let after = {
                let context = RequestContext::transaction(&transaction);
                let before = #module::Entity::find_by_id(id)
                    .lock_exclusive()
                    .one(&transaction).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(&context, &before).await? {
                    return Err(ApiError::NotFound);
                }
                if before.revision != expected {
                    return Err(ApiError::ConcurrentModification);
                }
                state.extensions.#hooks().before_update(&context, &before, &mut input).await?;
                validate_update(&input)?;
                let mut active = before.clone().into_active_model();
                #(#updates)*
                active.revision = Set(before.revision.checked_add(1)
                    .ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                let candidate = active.clone().try_into_model()?;
                if !state.extensions.#policy()
                    .can_update(&context, &before, &input, &candidate).await?
                {
                    return Err(ApiError::Forbidden);
                }
                let after = active.update(&transaction).await?;
                state.extensions.#hooks().after_update(&context, &before, &after).await?;
                after
            };
            transaction.commit().await?;
            run_after_commit(&state, crate::HookOperation::Update, &after).await;
            Ok((etag_header(&after), Json(after)))
        }
    }
}

fn delete_handler(
    module: &Ident,
    hooks: &Ident,
    policy: &Ident,
    parse_id: &TokenStream,
) -> TokenStream {
    quote! {
        async fn delete(
            State(state): State<AppState>,
            Path(id): Path<String>,
            headers: HeaderMap,
        ) -> Result<StatusCode, ApiError> {
            let expected = expected_revision(&headers)?;
            let id = #parse_id;
            let transaction = state.database.begin().await?;
            let deleted = {
                let context = RequestContext::transaction(&transaction);
                let model = #module::Entity::find_by_id(id)
                    .lock_exclusive()
                    .one(&transaction).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(&context, &model).await? {
                    return Err(ApiError::NotFound);
                }
                if model.revision != expected {
                    return Err(ApiError::ConcurrentModification);
                }
                if !state.extensions.#policy().can_delete(&context, &model).await? {
                    return Err(ApiError::Forbidden);
                }
                state.extensions.#hooks().before_delete(&context, &model).await?;
                let deleted = model.clone();
                model.delete(&transaction).await?;
                state.extensions.#hooks().after_delete(&context, &deleted).await?;
                deleted
            };
            transaction.commit().await?;
            run_after_commit(&state, crate::HookOperation::Delete, &deleted).await;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

fn helper_functions(module: &Ident, hooks: &Ident) -> TokenStream {
    quote! {
        fn expected_revision(headers: &HeaderMap) -> Result<i64, ApiError> {
            let value = headers.get(header::IF_MATCH).ok_or(ApiError::PreconditionRequired)?;
            let value = value.to_str().map_err(|_| ApiError::InvalidPrecondition)?;
            value.strip_prefix("\"rev-")
                .and_then(|value| value.strip_suffix('"'))
                .and_then(|value| value.parse().ok())
                .filter(|value| *value >= 1)
                .ok_or(ApiError::InvalidPrecondition)
        }

        fn etag_header(model: &#module::Model) -> [(header::HeaderName, String); 1] {
            [(header::ETAG, format!("\"rev-{}\"", model.revision))]
        }

        async fn run_after_commit(
            state: &AppState,
            operation: crate::HookOperation,
            model: &#module::Model,
        ) {
            let context = state.context();
            if let Err(error) = state.extensions.#hooks().after_commit(&context, operation, model).await {
                tracing::error!(?error, ?operation, entity = stringify!(#module), "after_commit hook failed");
            }
        }
    }
}
