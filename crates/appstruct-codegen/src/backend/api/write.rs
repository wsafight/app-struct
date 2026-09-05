use super::super::{access, module_name};
use crate::CodegenError;
use appstruct_ir::EntityIr;
use proc_macro2::{Ident, Literal, TokenStream};
use quote::quote;

pub(super) struct HandlerContext<'context> {
    pub module: &'context Ident,
    pub hooks: &'context Ident,
    pub policy: &'context Ident,
    pub parse_id: &'context TokenStream,
    pub primary: &'context Ident,
    pub soft_delete: bool,
    pub activity_resource: Option<&'context str>,
}

pub(super) fn handlers(
    entity: &EntityIr,
    context: &HandlerContext<'_>,
    create_values: &[TokenStream],
    active_default: Option<&TokenStream>,
    updates: &[TokenStream],
) -> Result<TokenStream, CodegenError> {
    let HandlerContext {
        module,
        hooks,
        policy,
        parse_id,
        primary,
        soft_delete,
        activity_resource,
    } = context;
    let read_scope = access::member_scope(entity, module, &entity.access.read)?;
    let create_allowed = access::create_allowed(entity, &entity.access.create)?;
    let update_allowed = access::update_allowed(entity, &entity.access.update)?;
    let delete_allowed = access::row_allowed(entity, &entity.access.delete)?;
    let event_prefix = module_name(entity);
    let resource = Literal::string(&entity.table_name);
    let create_event = Literal::string(&format!("{event_prefix}.created"));
    let update_event = Literal::string(&format!("{event_prefix}.updated"));
    let delete_event = Literal::string(&format!("{event_prefix}.deleted"));
    let read = read_handler(module, policy, parse_id, &read_scope);
    let create_audit = audit_event(entity, primary, "create");
    let update_audit = audit_event(entity, primary, "update");
    let delete_audit = audit_event(entity, primary, "delete");
    let create_activity = super::activity::write_event(*activity_resource, primary, "created");
    let update_activity = super::activity::write_event(*activity_resource, primary, "updated");
    let delete_activity = super::activity::write_event(*activity_resource, primary, "deleted");
    let create = create_handler(
        module,
        hooks,
        policy,
        create_values,
        active_default,
        &create_allowed,
        &create_audit,
        &create_activity,
        &create_event,
    );
    let update = update_handler(
        context,
        updates,
        &read_scope,
        &update_allowed,
        &update_audit,
        &update_activity,
        &update_event,
    );
    let delete = delete_handler(
        module,
        hooks,
        policy,
        parse_id,
        &read_scope,
        &delete_allowed,
        &delete_audit,
        &delete_activity,
        *soft_delete,
        &delete_event,
    );
    let helpers = helper_functions(module, hooks, primary, &resource);
    Ok(quote! {
        #read
        #create
        #update
        #delete
        #helpers
    })
}

fn read_handler(
    module: &Ident,
    policy: &Ident,
    parse_id: &TokenStream,
    read_scope: &TokenStream,
) -> TokenStream {
    quote! {
        async fn read(
            State(state): State<AppState>,
            Path(id): Path<String>,
            headers: HeaderMap,
        ) -> Result<([(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            let context = state.context(&headers).await?;
            let id = #parse_id;
            #read_scope
            let model = #module::Entity::find_by_id(id)
                .filter(access_condition)
                .one(&state.database).await?.ok_or(ApiError::NotFound)?;
            if !state.extensions.#policy().can_read(&context, &model).await? {
                return Err(ApiError::NotFound);
            }
            Ok((etag_header(&model), Json(redact_model(&context, model)?)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn create_handler(
    module: &Ident,
    hooks: &Ident,
    policy: &Ident,
    create_values: &[TokenStream],
    active_default: Option<&TokenStream>,
    create_allowed: &TokenStream,
    audit: &TokenStream,
    activity: &TokenStream,
    event: &Literal,
) -> TokenStream {
    quote! {
        async fn create(
            State(state): State<AppState>,
            headers: HeaderMap,
            Json(mut input): Json<CreateInput>,
        ) -> Result<(StatusCode, [(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            let context = state.mutation_context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            authorize_create_fields(&context, &input)?;
            state.extensions.#hooks().before_validate_create(&context, &mut input).await?;
            validate_create(&input)?;
            let transaction = state.database.begin().await?;
            let model = {
                let context = RequestContext::transaction_with_file(
                    &transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant,
                );
                state.extensions.#hooks().before_create(&context, &mut input).await?;
                authorize_create_fields(&context, &input)?;
                validate_create(&input)?;
                if !(#create_allowed) {
                    return Err(access_denied(&context));
                }
                if !state.extensions.#policy().can_create(&context, &input).await? {
                    return Err(ApiError::Forbidden);
                }
                let active = #module::ActiveModel { #(#create_values,)* #active_default };
                let model = active.insert(&transaction).await?;
                state.extensions.#hooks().after_create(&context, &model).await?;
                #audit
                #activity
                model
            };
            transaction.commit().await?;
            publish_realtime_event(&state, &context, #event, &model);
            run_after_commit(&state, crate::HookOperation::Create, &model, actor, tenant).await;
            Ok((StatusCode::CREATED, etag_header(&model), Json(redact_model(&context, model)?)))
        }
    }
}

fn update_handler(
    context: &HandlerContext<'_>,
    updates: &[TokenStream],
    read_scope: &TokenStream,
    update_allowed: &TokenStream,
    audit: &TokenStream,
    activity: &TokenStream,
    event: &Literal,
) -> TokenStream {
    let HandlerContext {
        module,
        hooks,
        policy,
        parse_id,
        ..
    } = context;
    quote! {
        async fn update(
            State(state): State<AppState>,
            Path(id): Path<String>,
            headers: HeaderMap,
            Json(mut input): Json<UpdateInput>,
        ) -> Result<([(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            let expected = expected_revision(&headers)?;
            let context = state.mutation_context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            authorize_update_fields(&context, &input)?;
            state.extensions.#hooks().before_validate_update(&context, &mut input).await?;
            validate_update(&input)?;
            let id = #parse_id;
            let transaction = state.database.begin().await?;
            let after = {
                let context = RequestContext::transaction_with_file(
                    &transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant,
                );
                #read_scope
                let before = #module::Entity::find_by_id(id)
                    .filter(access_condition)
                    .lock_exclusive()
                    .one(&transaction).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(&context, &before).await? {
                    return Err(ApiError::NotFound);
                }
                if before.revision != expected {
                    return Err(ApiError::ConcurrentModification);
                }
                state.extensions.#hooks().before_update(&context, &before, &mut input).await?;
                authorize_update_fields(&context, &input)?;
                validate_update(&input)?;
                let mut active = before.clone().into_active_model();
                #(#updates)*
                active.revision = Set(before.revision.checked_add(1)
                    .ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                let candidate = active.clone().try_into_model()?;
                if !(#update_allowed) {
                    return Err(access_denied(&context));
                }
                if !state.extensions.#policy()
                    .can_update(&context, &before, &input, &candidate).await?
                {
                    return Err(ApiError::Forbidden);
                }
                let after = active.update(&transaction).await?;
                state.extensions.#hooks().after_update(&context, &before, &after).await?;
                #audit
                #activity
                after
            };
            transaction.commit().await?;
            publish_realtime_event(&state, &context, #event, &after);
            run_after_commit(&state, crate::HookOperation::Update, &after, actor, tenant).await;
            Ok((etag_header(&after), Json(redact_model(&context, after)?)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn delete_handler(
    module: &Ident,
    hooks: &Ident,
    policy: &Ident,
    parse_id: &TokenStream,
    read_scope: &TokenStream,
    delete_allowed: &TokenStream,
    audit: &TokenStream,
    activity: &TokenStream,
    soft_delete: bool,
    event: &Literal,
) -> TokenStream {
    let delete_model = if soft_delete {
        quote! {
            let mut active = model.clone().into_active_model();
            active.deleted_at = Set(Some(chrono::Utc::now()));
            active.revision = Set(model.revision.checked_add(1)
                .ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
            active.update(&transaction).await?
        }
    } else {
        quote! {
            let deleted = model.clone();
            model.delete(&transaction).await?;
            deleted
        }
    };
    quote! {
        async fn delete(
            State(state): State<AppState>,
            Path(id): Path<String>,
            headers: HeaderMap,
        ) -> Result<StatusCode, ApiError> {
            let expected = expected_revision(&headers)?;
            let context = state.mutation_context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let id = #parse_id;
            let transaction = state.database.begin().await?;
            let deleted = {
                let context = RequestContext::transaction_with_file(
                    &transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant,
                );
                #read_scope
                let model = #module::Entity::find_by_id(id)
                    .filter(access_condition)
                    .lock_exclusive()
                    .one(&transaction).await?.ok_or(ApiError::NotFound)?;
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
                #activity
                deleted
            };
            transaction.commit().await?;
            publish_realtime_event(&state, &context, #event, &deleted);
            run_after_commit(&state, crate::HookOperation::Delete, &deleted, actor, tenant).await;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

fn audit_event(entity: &EntityIr, primary: &Ident, operation: &str) -> TokenStream {
    if !entity.audit_enabled {
        return TokenStream::new();
    }
    let entity_id = &entity.id.0;
    match operation {
        "create" => quote! {
            crate::audit::record(
                &transaction, &context, #entity_id, model.#primary.to_string(),
                "create", None, Some(&model),
            ).await?;
        },
        "update" => quote! {
            crate::audit::record(
                &transaction, &context, #entity_id, after.#primary.to_string(),
                "update", Some(&before), Some(&after),
            ).await?;
        },
        "delete" => quote! {
            crate::audit::record(
                &transaction, &context, #entity_id, deleted.#primary.to_string(),
                "delete", Some(&deleted), None,
            ).await?;
        },
        _ => unreachable!(),
    }
}

fn helper_functions(
    module: &Ident,
    hooks: &Ident,
    primary: &Ident,
    resource: &Literal,
) -> TokenStream {
    quote! {
        fn expected_revision(headers: &HeaderMap) -> Result<i64, ApiError> {
            let value = headers.get(header::IF_MATCH).ok_or(ApiError::PreconditionRequired)?;
            let value = value.to_str().map_err(|_| ApiError::InvalidPrecondition)?;
            appstruct_runtime::parse_revision_etag(value)
                .ok_or(ApiError::InvalidPrecondition)
        }

        fn etag_header(model: &#module::Model) -> [(header::HeaderName, String); 1] {
            [(header::ETAG, appstruct_runtime::revision_etag(model.revision))]
        }

        fn publish_realtime_event(
            state: &AppState,
            context: &RequestContext<'_>,
            event: &str,
            model: &#module::Model,
        ) {
            if let Err(error) = state.realtime.publish_resource_model(
                event, #resource, &model.#primary.to_string(), model,
                context.actor().map(|actor| actor.id),
                context.tenant(),
            ) {
                tracing::warn!(?error, event, "realtime event could not be published");
            }
        }

        async fn run_after_commit(
            state: &AppState,
            operation: crate::HookOperation,
            model: &#module::Model,
            actor: Option<crate::Actor>,
            tenant: Option<crate::TenantId>,
        ) {
            let context = RequestContext::connection_with_services(
                &state.database, &state.mail, &state.file, &state.realtime, actor, tenant,
            );
            if let Err(error) = state.extensions.#hooks().after_commit(&context, operation, model).await {
                tracing::error!(?error, ?operation, entity = stringify!(#module), "after_commit hook failed");
            }
        }
    }
}
