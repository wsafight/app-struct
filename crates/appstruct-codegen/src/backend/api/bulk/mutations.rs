use super::{BulkContext, audit_event};
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn restore(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        bulk_parse_id,
        policy,
        hooks,
        update_allowed,
        primary,
        entity_id,
        restore_scope,
        audit_enabled,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "restore");
    let update_event = format!("{module}.updated");
    quote! {
        async fn restore(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkDeleteInput>,
        ) -> Result<Json<BulkResult>, ApiError> {
            let context = state.mutation_context(&headers).await?;
            if !bulk_request_size_is_valid(input.ids.len(), input.expected_revisions.len()) { return Err(ApiError::InvalidQuery(format!("bulk requests must contain between 1 and {MAX_BULK_ITEMS} ids"))); }
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            let mut restored = Vec::new();
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = match #bulk_parse_id {
                    Ok(id) => id,
                    Err(error) => { result.failed.push(error.into_bulk_failure(id_text)); continue; }
                };
                let savepoint = transaction.begin().await?;
                let outcome: Result<#module::Model, ApiError> = async {
                    let context = RequestContext::transaction_with_file(&savepoint, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                    let mut select = #module::Entity::find_by_id(id);
                    #restore_scope
                    let before = select.lock_exclusive().one(&savepoint).await?.ok_or(ApiError::NotFound)?;
                    if !state.extensions.#policy().can_read(&context, &before).await? {
                        return Err(ApiError::NotFound);
                    }
                    if before.revision != expected {
                        return Err(ApiError::ConcurrentModification);
                    }
                    let mut input = crate::api::#module::UpdateInput::default();
                    state.extensions.#hooks().before_update(&context, &before, &mut input).await?;
                    authorize_update_fields(&context, &input)?;
                    validate_update(&input)?;
                    let mut active = before.clone().into_active_model();
                    active.deleted_at = Set(None);
                    active.revision = Set(before.revision.checked_add(1).ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                    let candidate = active.clone().try_into_model()?;
                    if !(#update_allowed) {
                        return Err(access_denied(&context));
                    }
                    if !state.extensions.#policy().can_update(&context, &before, &input, &candidate).await? {
                        return Err(ApiError::Forbidden);
                    }
                    let after = active.update(&savepoint).await?;
                    state.extensions.#hooks().after_update(&context, &before, &after).await?;
                    #audit
                    Ok(after)
                }.await;
                match outcome {
                    Ok(model) => {
                        savepoint.commit().await?;
                        result.succeeded.push(id_text.clone());
                        restored.push(model);
                    }
                    Err(error) => {
                        savepoint.rollback().await?;
                        result.failed.push(error.into_bulk_failure(id_text));
                    }
                }
            }
            transaction.commit().await?;
            for model in &restored {
                publish_realtime_event(&state, &context, #update_event, model);
                run_after_commit(&state, crate::HookOperation::Update, model, actor.clone(), tenant).await;
            }
            Ok(Json(result))
        }
    }
}

pub(super) fn update(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        primary,
        bulk_parse_id,
        hooks,
        policy,
        update_allowed,
        updates,
        read_scope,
        audit_enabled,
        entity_id,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "update");
    let update_event = format!("{module}.updated");
    quote! {
        async fn bulk_update(
            State(state): State<AppState>, headers: HeaderMap,
            Json(mut input): Json<BulkUpdateInput<UpdateInput>>,
        ) -> Result<Json<BulkResult>, ApiError> {
            let context = state.mutation_context(&headers).await?;
            if !bulk_request_size_is_valid(input.ids.len(), input.expected_revisions.len()) { return Err(ApiError::InvalidQuery(format!("bulk requests must contain between 1 and {MAX_BULK_ITEMS} ids"))); }
            authorize_update_fields(&context, &input.patch)?;
            state.extensions.#hooks().before_validate_update(&context, &mut input.patch).await?;
            validate_update(&input.patch)?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            let mut updated = Vec::new();
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = match #bulk_parse_id {
                    Ok(id) => id,
                    Err(error) => { result.failed.push(error.into_bulk_failure(id_text)); continue; }
                };
                let savepoint = transaction.begin().await?;
                let outcome: Result<#module::Model, ApiError> = async {
                    let context = RequestContext::transaction_with_file(&savepoint, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                    let mut select = #module::Entity::find_by_id(id);
                    #read_scope
                    let before = select.lock_exclusive().one(&savepoint).await?.ok_or(ApiError::NotFound)?;
                    if !state.extensions.#policy().can_read(&context, &before).await? {
                        return Err(ApiError::NotFound);
                    }
                    if before.revision != expected {
                        return Err(ApiError::ConcurrentModification);
                    }
                    let mut input = input.patch.clone();
                    state.extensions.#hooks().before_update(&context, &before, &mut input).await?;
                    authorize_update_fields(&context, &input)?;
                    validate_update(&input)?;
                    let mut active = before.clone().into_active_model();
                    #(#updates)*
                    active.revision = Set(before.revision.checked_add(1).ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                    let candidate = active.clone().try_into_model()?;
                    if !(#update_allowed) {
                        return Err(access_denied(&context));
                    }
                    if !state.extensions.#policy().can_update(&context, &before, &input, &candidate).await? {
                        return Err(ApiError::Forbidden);
                    }
                    let after = active.update(&savepoint).await?;
                    state.extensions.#hooks().after_update(&context, &before, &after).await?;
                    #audit
                    Ok(after)
                }.await;
                match outcome {
                    Ok(model) => {
                        savepoint.commit().await?;
                        result.succeeded.push(id_text.clone());
                        updated.push(model);
                    }
                    Err(error) => {
                        savepoint.rollback().await?;
                        result.failed.push(error.into_bulk_failure(id_text));
                    }
                }
            }
            transaction.commit().await?;
            for model in &updated {
                publish_realtime_event(&state, &context, #update_event, model);
                run_after_commit(&state, crate::HookOperation::Update, model, actor.clone(), tenant).await;
            }
            Ok(Json(result))
        }
    }
}

pub(super) fn delete(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        bulk_parse_id,
        hooks,
        policy,
        read_scope,
        delete_allowed,
        primary,
        audit_enabled,
        entity_id,
        soft_delete,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "delete");
    let delete_event = format!("{module}.deleted");
    let delete_model = if *soft_delete {
        quote! {
            let mut active = model.clone().into_active_model();
            active.deleted_at = Set(Some(chrono::Utc::now()));
            active.revision = Set(model.revision.checked_add(1)
                .ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
            active.update(&savepoint).await?
        }
    } else {
        quote! {
            let deleted = model.clone();
            model.delete(&savepoint).await?;
            deleted
        }
    };
    quote! {
        async fn bulk_delete(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkDeleteInput>,
        ) -> Result<Json<BulkResult>, ApiError> {
            let context = state.mutation_context(&headers).await?;
            if !bulk_request_size_is_valid(input.ids.len(), input.expected_revisions.len()) { return Err(ApiError::InvalidQuery(format!("bulk requests must contain between 1 and {MAX_BULK_ITEMS} ids"))); }
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            let mut deleted_models = Vec::new();
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = match #bulk_parse_id {
                    Ok(id) => id,
                    Err(error) => { result.failed.push(error.into_bulk_failure(id_text)); continue; }
                };
                let savepoint = transaction.begin().await?;
                let outcome: Result<#module::Model, ApiError> = async {
                    let context = RequestContext::transaction_with_file(&savepoint, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                    let mut select = #module::Entity::find_by_id(id);
                    #read_scope
                    let model = select.lock_exclusive().one(&savepoint).await?.ok_or(ApiError::NotFound)?;
                    if !state.extensions.#policy().can_read(&context, &model).await? {
                        return Err(ApiError::NotFound);
                    }
                    if model.revision != expected {
                        return Err(ApiError::ConcurrentModification);
                    }
                    if !(#delete_allowed) {
                        return Err(access_denied(&context));
                    }
                    if !state.extensions.#policy().can_delete(&context, &model).await? {
                        return Err(ApiError::Forbidden);
                    }
                    state.extensions.#hooks().before_delete(&context, &model).await?;
                    let deleted = { #delete_model };
                    state.extensions.#hooks().after_delete(&context, &deleted).await?;
                    #audit
                    Ok(deleted)
                }.await;
                match outcome {
                    Ok(model) => {
                        savepoint.commit().await?;
                        result.succeeded.push(id_text.clone());
                        deleted_models.push(model);
                    }
                    Err(error) => {
                        savepoint.rollback().await?;
                        result.failed.push(error.into_bulk_failure(id_text));
                    }
                }
            }
            transaction.commit().await?;
            for model in &deleted_models {
                publish_realtime_event(&state, &context, #delete_event, model);
                run_after_commit(&state, crate::HookOperation::Delete, model, actor.clone(), tenant).await;
            }
            Ok(Json(result))
        }
    }
}
