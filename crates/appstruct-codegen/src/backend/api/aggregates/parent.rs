use super::super::super::{access, module_name, parse_ident};
use crate::CodegenError;
use appstruct_ir::{AggregateIr, AppIr, EntityIr, GeneratedValueIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

#[allow(clippy::too_many_lines)]
pub(super) fn support(
    ir: &AppIr,
    parent: &EntityIr,
    aggregate: &AggregateIr,
) -> Result<(TokenStream, TokenStream), CodegenError> {
    let module = parse_ident(&module_name(parent))?;
    let hooks = format_ident!("{}_hooks", module_name(parent));
    let policy = format_ident!("{}_policy", module_name(parent));
    let child = ir
        .entities
        .iter()
        .find(|entity| entity.id == aggregate.child)
        .expect("validated child");
    let child_module = parse_ident(&module_name(child))?;
    let child_primary = parse_ident(&super::super::primary_key(child)?.rust_name)?;
    let primary = parse_ident(&super::super::primary_key(parent)?.rust_name)?;
    let create_fn = format_ident!("create_with_aggregate_{}", aggregate.name);
    let create_input_ty = format_ident!("CreateWithAggregateInput_{}", aggregate.name);
    let read = format_ident!("read_aggregate_{}", aggregate.name);
    let write = format_ident!("write_aggregate_{}", aggregate.name);
    let create_path = format!("/_aggregates/{}", aggregate.name);
    let path = format!("/{{id}}/_aggregates/{}", aggregate.name);
    let limit = u64::from(aggregate.max_items);
    let scope = access::member_scope(parent, &module, &parent.access.read)?;
    let create_allowed = access::create_allowed(parent, &parent.access.create)?;
    let allowed = access::update_allowed(parent, &parent.access.update)?;
    let create_values = super::super::create_values(parent)?;
    let active_default = parent
        .fields
        .iter()
        .any(|field| matches!(field.generated, Some(GeneratedValueIr::AutoIncrement)))
        .then(|| quote! { ..Default::default() });
    let (parent_file_create, _, _) =
        super::super::file_field_lifecycle(parent, &primary, &quote! { &context })?;
    let guard = parent.workflow_field().map(|field| {
        let field = parse_ident(&field.rust_name).expect("validated field");
        let states = &aggregate.states;
        quote! { if ![#(#states),*].contains(&before.#field.as_str()) { return Err(ApiError::InvalidWorkflowState); } }
    });
    let create_guard = parent.workflow_field().map(|field| {
        let field = parse_ident(&field.rust_name).expect("validated field");
        let states = &aggregate.states;
        quote! {
            if !creates.is_empty() && ![#(#states),*].contains(&active.#field.as_str()) {
                return Err(ApiError::InvalidWorkflowState);
            }
        }
    });
    let create_event = format!("{}.created", module_name(parent));
    let event = format!("{}.aggregate.{}", module_name(parent), aggregate.name);
    let entity_id = &parent.id.0;
    let create_audit = parent.audit_enabled.then(|| {
        quote! {
            crate::audit::record(&transaction, &context, #entity_id, model.#primary.to_string(), "create", None, Some(&model)).await?;
        }
    });
    let create_activity = ir.activity.resource_for_entity(&parent.id).map(|resource| {
        let resource = &resource.resource;
        quote! { crate::activity::record_system_event(&transaction, &context, #resource, model.#primary.to_string(), "created").await?; }
    });
    let audit = parent.audit_enabled.then(|| {
        quote! {
            crate::audit::record(&transaction, &context, #entity_id, after.#primary.to_string(), #event, Some(&before), Some(&after)).await?;
        }
    });
    let activity = ir.activity.resource_for_entity(&parent.id).map(|resource| {
        let resource = &resource.resource;
        quote! { crate::activity::record_system_event(&transaction, &context, #resource, after.#primary.to_string(), #event).await?; }
    });
    let tokens = quote! {
        #[allow(non_camel_case_types)]
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct #create_input_ty {
            parent: CreateInput,
            #[serde(default)]
            creates: Vec<super::#child_module::aggregate_rows::CreateRow>,
        }
        async fn #create_fn(
            State(state): State<AppState>,
            headers: HeaderMap,
            Json(body): Json<#create_input_ty>,
        ) -> Result<(StatusCode, [(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            use super::#child_module::aggregate_rows as rows;
            let request = state.mutation_context(&headers).await?;
            rows::validate_creates(&body.creates, #limit as usize)?;
            let mut input = body.parent;
            let creates = body.creates;
            authorize_create_fields(&request, &input)?;
            state.extensions.#hooks().before_validate_create(&request, &mut input).await?;
            validate_create(&input)?;
            let transaction = state.database.begin().await?;
            let mut events = Vec::new();
            let (model, response) = {
                let context = RequestContext::transaction_with_file(
                    &transaction, &state.mail, &state.file, &state.realtime, request.shared_actor(), request.tenant(),
                );
                state.extensions.#hooks().before_create(&context, &mut input).await?;
                authorize_create_fields(&context, &input)?;
                validate_create(&input)?;
                if !(#create_allowed) { return Err(access_denied(&context)); }
                if !state.extensions.#policy().can_create(&context, &input).await? { return Err(ApiError::Forbidden); }
                let active = #module::Model { #(#create_values,)* #active_default };
                #create_guard
                validate_relations(&state, &context, &active).await?;
                validate_candidate(&active)?;
                let model = active.insert(&transaction).await?;
                #parent_file_create
                state.extensions.#hooks().after_create(&context, &model).await?;
                #create_audit
                #create_activity
                let mut created = BTreeMap::new();
                for row in creates {
                    let path = format!("creates.{}", row.key);
                    let child = rows::create(&state, &context, model.#primary, row.input).await.map_err(|error| rows::row_error(error, &path))?;
                    created.insert(row.key, child.#child_primary.to_string());
                    events.push((crate::HookOperation::Create, child));
                }
                let collection = rows::collection(&state, &context, model.#primary, #limit).await?;
                let response = serde_json::json!({ "parent": redact_model(&context, model.clone())?, "rows": collection, "created": created });
                (model, response)
            };
            transaction.commit().await?;
            publish_realtime_event(&state, &request, #create_event, &model);
            run_after_commit(&state, crate::HookOperation::Create, &model, request.shared_actor(), request.tenant()).await;
            for (operation, child) in events {
                rows::committed(&state, &request, operation, &child).await;
            }
            Ok((StatusCode::CREATED, etag_header(&model), Json(response)))
        }
        async fn #read(State(state): State<AppState>, Path(id): Path<uuid::Uuid>, headers: HeaderMap) -> Result<([(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            let request = state.context(&headers).await?;
            let transaction = state.database.begin().await?;
            let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, request.shared_actor(), request.tenant());
            #scope
            let parent = #module::select_by_id(id).filter(access_condition).lock_shared().one(&transaction).await?.ok_or(ApiError::NotFound)?;
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
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, request.shared_actor(), request.tenant());
                #scope
                let before = #module::select_by_id(id).filter(access_condition).lock_exclusive().one(&transaction).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(&context, &before).await? { return Err(ApiError::NotFound); }
                if before.revision != expected { return Err(ApiError::ConcurrentModification); }
                #guard
                let mut active = before.clone();
                active.revision = before.revision.checked_add(1).ok_or(ApiError::Internal)?;
                let candidate = &active;
                if !(#allowed) || !state.extensions.#policy().can_update(&context, &before, &UpdateInput::default(), candidate).await? {
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
                if !state.extensions.#policy().can_update(&context, &before, &UpdateInput::default(), candidate).await? { return Err(ApiError::Forbidden); }
                validate_candidate(&active)?;
                let after = active.update(&transaction).await?;
                #audit
                #activity
                let response = serde_json::json!({ "parent": redact_model(&context, after.clone())?, "rows": collection, "created": created });
                (after, response)
            };
            transaction.commit().await?;
            publish_realtime_event(&state, &request, #event, &after);
            run_after_commit(&state, crate::HookOperation::Update, &after, request.shared_actor(), request.tenant()).await;
            for (operation, model) in events { rows::committed(&state, &request, operation, &model).await; }
            Ok((etag_header(&after), Json(response)))
        }
    };
    Ok((
        quote! {
            .route(#create_path, axum::routing::post(#create_fn))
            .route(#path, get(#read).post(#write))
        },
        tokens,
    ))
}
