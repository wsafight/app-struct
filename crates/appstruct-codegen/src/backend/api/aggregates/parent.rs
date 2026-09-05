use super::super::super::{access, module_name, parse_ident};
use crate::CodegenError;
use appstruct_ir::{AggregateIr, AppIr, EntityIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

#[allow(clippy::too_many_lines)]
pub(super) fn support(
    ir: &AppIr,
    parent: &EntityIr,
    aggregate: &AggregateIr,
) -> Result<(TokenStream, TokenStream), CodegenError> {
    let module = parse_ident(&module_name(parent))?;
    let policy = format_ident!("{}_policy", module_name(parent));
    let child = ir
        .entities
        .iter()
        .find(|entity| entity.id == aggregate.child)
        .expect("validated child");
    let child_module = parse_ident(&module_name(child))?;
    let child_primary = parse_ident(&super::super::primary_key(child)?.rust_name)?;
    let primary = parse_ident(&super::super::primary_key(parent)?.rust_name)?;
    let read = format_ident!("read_aggregate_{}", aggregate.name);
    let write = format_ident!("write_aggregate_{}", aggregate.name);
    let path = format!("/{{id}}/_aggregates/{}", aggregate.name);
    let limit = u64::from(aggregate.max_items);
    let scope = access::member_scope(parent, &module, &parent.access.read)?;
    let allowed = access::update_allowed(parent, &parent.access.update)?;
    let guard = parent.workflow_field().map(|field| {
        let field = parse_ident(&field.rust_name).expect("validated field");
        let states = &aggregate.states;
        quote! { if ![#(#states),*].contains(&before.#field.as_str()) { return Err(ApiError::InvalidWorkflowState); } }
    });
    let event = format!("{}.aggregate.{}", module_name(parent), aggregate.name);
    let entity_id = &parent.id.0;
    let audit = parent.audit_enabled.then(|| quote! {
        crate::audit::record(&transaction, &context, #entity_id, after.#primary.to_string(), #event, Some(&before), Some(&after)).await?;
    });
    let activity = ir.activity.resource_for_entity(&parent.id).map(|resource| {
        let resource = &resource.resource;
        quote! { crate::activity::record_system_event(&transaction, &context, #resource, after.#primary.to_string(), #event).await?; }
    });
    let tokens = quote! {
        async fn #read(State(state): State<AppState>, Path(id): Path<uuid::Uuid>, headers: HeaderMap) -> Result<([(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            let request = state.context(&headers).await?;
            let transaction = state.database.begin().await?;
            let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, request.actor().cloned(), request.tenant());
            #scope
            let parent = #module::Entity::find_by_id(id).filter(access_condition).lock_shared().one(&transaction).await?.ok_or(ApiError::NotFound)?;
            if !state.extensions.#policy().can_read(&context, &parent).await? { return Err(ApiError::NotFound); }
            let rows = super::#child_module::aggregate_rows::collection(&state, &context, id, #limit).await?;
            let etag = etag_header(&parent);
            let response = serde_json::json!({ "parent": redact_model(&context, parent)?, "rows": rows, "created": {} });
            transaction.commit().await?;
            Ok((etag, Json(response)))
        }
        async fn #write(State(state): State<AppState>, Path(id): Path<uuid::Uuid>, headers: HeaderMap, Json(mut input): Json<super::#child_module::aggregate_rows::Batch>) -> Result<([(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            use super::#child_module::aggregate_rows as rows;
            let expected = expected_revision(&headers)?;
            let request = state.mutation_context(&headers).await?;
            input.validate(#limit as usize)?;
            let transaction = state.database.begin().await?;
            let mut events = Vec::new();
            let (after, response) = {
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, request.actor().cloned(), request.tenant());
                #scope
                let before = #module::Entity::find_by_id(id).filter(access_condition).lock_exclusive().one(&transaction).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(&context, &before).await? { return Err(ApiError::NotFound); }
                if before.revision != expected { return Err(ApiError::ConcurrentModification); }
                #guard
                let mut active = before.clone().into_active_model();
                active.revision = Set(before.revision.checked_add(1).ok_or(ApiError::Internal)?);
                let candidate = active.clone().try_into_model()?;
                if !(#allowed) || !state.extensions.#policy().can_update(&context, &before, &UpdateInput::default(), &candidate).await? {
                    return Err(access_denied(&context));
                }
                let mut created = BTreeMap::new();
                for row in input.deletes {
                    let path = format!("deletes.{}", row.id);
                    let model = rows::delete(&state, &context, id, row).await.map_err(|error| rows::row_error(error, &path))?;
                    events.push((crate::HookOperation::Delete, model));
                }
                for row in input.updates {
                    let path = format!("updates.{}", row.id);
                    let model = rows::update(&state, &context, id, row).await.map_err(|error| rows::row_error(error, &path))?;
                    events.push((crate::HookOperation::Update, model));
                }
                for row in input.creates {
                    let path = format!("creates.{}", row.key);
                    let model = rows::create(&state, &context, id, row.input).await.map_err(|error| rows::row_error(error, &path))?;
                    created.insert(row.key, model.#child_primary.to_string());
                    events.push((crate::HookOperation::Create, model));
                }
                let collection = rows::collection(&state, &context, id, #limit).await?;
                if !state.extensions.#policy().can_update(&context, &before, &UpdateInput::default(), &candidate).await? { return Err(ApiError::Forbidden); }
                let after = active.update(&transaction).await?;
                #audit
                #activity
                let response = serde_json::json!({ "parent": redact_model(&context, after.clone())?, "rows": collection, "created": created });
                (after, response)
            };
            transaction.commit().await?;
            publish_realtime_event(&state, &request, #event, &after);
            run_after_commit(&state, crate::HookOperation::Update, &after, request.actor().cloned(), request.tenant()).await;
            for (operation, model) in events { rows::committed(&state, &request, operation, &model).await; }
            Ok((etag_header(&after), Json(response)))
        }
    };
    Ok((quote! { .route(#path, get(#read).post(#write)) }, tokens))
}
