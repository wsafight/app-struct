use super::super::access;
use crate::CodegenError;
use appstruct_ir::EntityIr;
use proc_macro2::{Ident, TokenStream};
use quote::quote;

mod csv;

pub(super) struct SourceContext<'context> {
    pub module: &'context Ident,
    pub parse_id: &'context TokenStream,
    pub primary: &'context Ident,
    pub hooks: &'context Ident,
    pub policy: &'context Ident,
    pub list_scope: &'context TokenStream,
    pub create_allowed: &'context TokenStream,
    pub delete_allowed: &'context TokenStream,
    pub update_allowed: &'context TokenStream,
    pub create_values: &'context [TokenStream],
    pub active_default: Option<&'context TokenStream>,
    pub updates: &'context [TokenStream],
}

pub(super) struct BulkContext<'context> {
    pub module: &'context Ident,
    pub primary: &'context Ident,
    pub primary_column: &'context Ident,
    pub parse_id: &'context TokenStream,
    pub hooks: &'context Ident,
    pub policy: &'context Ident,
    pub list_scope: &'context TokenStream,
    pub delete_allowed: &'context TokenStream,
    pub update_allowed: &'context TokenStream,
    pub create_allowed: &'context TokenStream,
    pub create_values: &'context [TokenStream],
    pub active_default: Option<&'context TokenStream>,
    pub updates: &'context [TokenStream],
    pub entity_id: &'context str,
    pub audit_enabled: bool,
    pub soft_delete: bool,
    pub tenant_scoped: bool,
    pub trash_scope: &'context TokenStream,
}

pub(super) fn source(
    entity: &EntityIr,
    source: &SourceContext<'_>,
) -> Result<TokenStream, CodegenError> {
    let SourceContext {
        module,
        parse_id,
        primary,
        hooks,
        policy,
        list_scope,
        create_allowed,
        delete_allowed,
        update_allowed,
        create_values,
        active_default,
        updates,
    } = source;
    let trash_scope = access::trash_scope(entity, module, &entity.access.list)?;
    let primary_column = super::super::query::helpers::column_ident(
        entity
            .fields
            .iter()
            .find(|field| field.primary_key)
            .ok_or_else(|| CodegenError::new("entity must define a primary key"))?,
    )?;
    let context = BulkContext {
        module,
        primary,
        primary_column: &primary_column,
        parse_id,
        hooks,
        policy,
        list_scope,
        delete_allowed,
        update_allowed,
        create_allowed,
        create_values,
        active_default: *active_default,
        updates,
        entity_id: &entity.id.0,
        audit_enabled: entity.audit_enabled,
        soft_delete: entity.views.soft_delete,
        tenant_scoped: entity.tenant_scoped,
        trash_scope: &trash_scope,
    };
    let update = bulk_update(&context);
    let delete = bulk_delete(&context);
    let export = csv::export(entity, module, policy, list_scope);
    let import = csv::import(entity, &context);
    let restore = context.soft_delete.then(|| restore_handler(&context));
    let trash = context.soft_delete.then(|| trash_handler(&context));
    Ok(quote! {
        use appstruct_runtime::{
            BulkDeleteInput, BulkResult, BulkUpdateInput, bulk_failure, csv_escape,
            csv_json_value, parse_csv_rows,
        };

        #update
        #delete
        #export
        #import
        #restore
        #trash
    })
}

fn trash_handler(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        primary_column,
        policy,
        trash_scope,
        ..
    } = context;
    quote! {
        async fn trash(
            State(state): State<AppState>, headers: HeaderMap,
            axum::extract::Query(query): axum::extract::Query<ListQuery>,
        ) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? {
                return Err(access_denied(&context));
            }
            if query.cursor.is_some() || query.limit.is_some() || query.sort.is_some() || query.q.is_some() || !query.filters.is_empty() {
                return Err(ApiError::InvalidQuery(
                    "trash only supports `page` and `page_size` pagination".to_owned()
                ));
            }
            let page = query.page.unwrap_or(1);
            let page_size = query.page_size.unwrap_or(25);
            if page == 0 || !(1..=100).contains(&page_size) {
                return Err(ApiError::InvalidQuery(
                    "`page` must be at least 1 and `page_size` must be between 1 and 100".to_owned()
                ));
            }
            let mut select = #module::Entity::find();
            #trash_scope
            select = select.order_by_asc(#module::Column::#primary_column);
            let total = select.clone().count(&state.database).await?;
            let data = select.paginate(&state.database, page_size).fetch_page(page - 1).await?
                .into_iter().map(|model| redact_model(&context, model))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Json(ListResponse {
                data,
                meta: ListMeta::Page { page, page_size, total },
            }))
        }
    }
}

fn restore_handler(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        parse_id,
        policy,
        hooks,
        update_allowed,
        primary,
        entity_id,
        tenant_scoped,
        audit_enabled,
        ..
    } = context;
    let tenant_filter = if *tenant_scoped {
        quote! { select = select.filter(#module::Column::TenantId.eq(context.require_tenant()?)); }
    } else {
        TokenStream::new()
    };
    let select_decl = if *tenant_scoped {
        quote! { let mut select = #module::Entity::find_by_id(id); }
    } else {
        quote! { let select = #module::Entity::find_by_id(id); }
    };
    let audit = audit_event(*audit_enabled, entity_id, primary, "update");
    quote! {
        async fn restore(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkDeleteInput>,
        ) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = { let id = id_text.clone(); #parse_id };
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                #select_decl
                #tenant_filter
                let Some(before) = select.lock_exclusive().one(&transaction).await? else {
                    result.failed.push(bulk_failure(id_text, "not_found", "record was not found"));
                    continue;
                };
                if before.deleted_at.is_none() || before.revision != expected {
                    result.failed.push(bulk_failure(id_text, "invalid_restore", "record is not in the trash or revision is stale"));
                    continue;
                }
                let mut active = before.clone().into_active_model();
                active.deleted_at = Set(None);
                active.revision = Set(before.revision.checked_add(1).ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                let candidate = active.clone().try_into_model()?;
                if !state.extensions.#policy().can_read(&context, &before).await? || !(#update_allowed) || !state.extensions.#policy().can_update(&context, &before, &crate::api::#module::UpdateInput::default(), &candidate).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record restore is not allowed"));
                    continue;
                }
                state.extensions.#hooks().before_update(&context, &before, &mut crate::api::#module::UpdateInput::default()).await?;
                let after = active.update(&transaction).await?;
                state.extensions.#hooks().after_update(&context, &before, &after).await?;
                #audit
                result.succeeded.push(id_text.clone());
            }
            transaction.commit().await?;
            Ok(Json(result))
        }
    }
}

fn bulk_update(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        primary,
        parse_id,
        hooks,
        policy,
        update_allowed,
        updates,
        list_scope,
        audit_enabled,
        entity_id,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "update");
    quote! {
        async fn bulk_update(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkUpdateInput<UpdateInput>>,
        ) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            authorize_update_fields(&context, &input.patch)?;
            validate_update(&input.patch)?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = match id_text.parse::<String>() {
                    Ok(_) => { let id = id_text.clone(); #parse_id }
                    Err(_) => { result.failed.push(bulk_failure(id_text, "invalid_id", "invalid record id")); continue; }
                };
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                let mut select = #module::Entity::find_by_id(id);
                #list_scope
                let model = select.lock_exclusive().one(&transaction).await?;
                let Some(before) = model else {
                    result.failed.push(bulk_failure(id_text, "not_found", "record was not found"));
                    continue;
                };
                if before.revision != expected {
                    result.failed.push(bulk_failure(id_text, "concurrent_modification", "record revision is stale"));
                    continue;
                }
                if !state.extensions.#policy().can_read(&context, &before).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record update is not allowed"));
                    continue;
                }
                let mut input = input.patch.clone();
                state.extensions.#hooks().before_update(&context, &before, &mut input).await?;
                authorize_update_fields(&context, &input)?;
                validate_update(&input)?;
                let mut active = before.clone().into_active_model();
                #(#updates)*
                active.revision = Set(before.revision.checked_add(1).ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                let candidate = active.clone().try_into_model()?;
                if !(#update_allowed) || !state.extensions.#policy().can_update(&context, &before, &input, &candidate).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record update is not allowed by policy"));
                    continue;
                }
                let after = active.update(&transaction).await?;
                state.extensions.#hooks().after_update(&context, &before, &after).await?;
                #audit
                result.succeeded.push(id_text.clone());
            }
            transaction.commit().await?;
            Ok(Json(result))
        }
    }
}

fn bulk_delete(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        parse_id,
        hooks,
        policy,
        list_scope,
        delete_allowed,
        primary,
        audit_enabled,
        entity_id,
        soft_delete,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "delete");
    let delete_model = if *soft_delete {
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
        async fn bulk_delete(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkDeleteInput>,
        ) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = { let id = id_text.clone(); #parse_id };
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                let mut select = #module::Entity::find_by_id(id);
                #list_scope
                let Some(model) = select.lock_exclusive().one(&transaction).await? else {
                    result.failed.push(bulk_failure(id_text, "not_found", "record was not found"));
                    continue;
                };
                if model.revision != expected || !state.extensions.#policy().can_read(&context, &model).await? || !(#delete_allowed) || !state.extensions.#policy().can_delete(&context, &model).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record delete is not allowed"));
                    continue;
                }
                state.extensions.#hooks().before_delete(&context, &model).await?;
                let deleted = { #delete_model };
                state.extensions.#hooks().after_delete(&context, &deleted).await?;
                #audit
                result.succeeded.push(id_text.clone());
            }
            transaction.commit().await?;
            Ok(Json(result))
        }
    }
}

fn audit_event(enabled: bool, entity_id: &str, primary: &Ident, operation: &str) -> TokenStream {
    if !enabled {
        return TokenStream::new();
    }
    let entity_id = entity_id.to_owned();
    match operation {
        "create" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, model.#primary.to_string(), "create", None, Some(&model)).await?; }
        }
        "update" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, after.#primary.to_string(), "update", Some(&before), Some(&after)).await?; }
        }
        "delete" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, deleted.#primary.to_string(), "delete", Some(&deleted), None).await?; }
        }
        "restore" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, after.#primary.to_string(), "restore", Some(&before), Some(&after)).await?; }
        }
        _ => TokenStream::new(),
    }
}
