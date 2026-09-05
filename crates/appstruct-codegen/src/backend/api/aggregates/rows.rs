use super::super::super::{access, module_name, parse_ident};
use crate::CodegenError;
use appstruct_ir::{AggregateIr, AppIr, EntityIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

#[allow(clippy::too_many_lines)]
pub(super) fn support(
    ir: &AppIr,
    entity: &EntityIr,
    aggregate: &AggregateIr,
) -> Result<TokenStream, CodegenError> {
    let module = parse_ident(&module_name(entity))?;
    let hooks = format_ident!("{}_hooks", module_name(entity));
    let policy = format_ident!("{}_policy", module_name(entity));
    let primary = parse_ident(&super::super::primary_key(entity)?.rust_name)?;
    let relation = entity
        .fields
        .iter()
        .find(|field| field.id == aggregate.relation)
        .expect("validated relation");
    let relation_name = parse_ident(&relation.rust_name)?;
    let relation_column = super::super::super::query::helpers::column_ident(relation)?;
    let primary_column =
        super::super::super::query::helpers::column_ident(super::super::primary_key(entity)?)?;
    let scope = access::member_scope(entity, &module, &entity.access.read)?;
    let create_allowed = access::create_allowed(entity, &entity.access.create)?;
    let update_allowed = access::update_allowed(entity, &entity.access.update)?;
    let delete_allowed = access::row_allowed(entity, &entity.access.delete)?;
    let creates = super::super::create_values(entity)?;
    let updates = super::super::update_values(entity)?;
    let input = super::input::support(entity, aggregate);
    let relations = super::relations::support(ir, entity)?;
    let create_event = events(ir, entity, &primary, "create", &quote! { model }, None);
    let update_event = events(
        ir,
        entity,
        &primary,
        "update",
        &quote! { after },
        Some(&quote! { before }),
    );
    let delete_event = events(ir, entity, &primary, "delete", &quote! { model }, None);
    let event_prefix = module_name(entity);
    Ok(quote! {
        #input
        #relations
        pub async fn collection(state: &AppState, context: &RequestContext<'_>, parent: uuid::Uuid, limit: u64) -> Result<Vec<serde_json::Value>, ApiError> {
            use sea_orm::PaginatorTrait as _;
            let count = #module::Entity::find().filter(#module::Column::#relation_column.eq(parent)).count(context).await?;
            if count > limit { return Err(invalid("Collection exceeds max_items")); }
            #scope
            let models = #module::Entity::find().filter(access_condition)
                .filter(#module::Column::#relation_column.eq(parent))
                .order_by_asc(#module::Column::#primary_column).all(context).await?;
            let mut rows = Vec::new();
            for model in models {
                if state.extensions.#policy().can_read(context, &model).await? {
                    rows.push(redact_model(context, model)?);
                }
            }
            Ok(rows)
        }
        async fn load(state: &AppState, context: &RequestContext<'_>, parent: uuid::Uuid, id: uuid::Uuid, revision: i64) -> Result<#module::Model, ApiError> {
            #scope
            let model = #module::Entity::find_by_id(id).filter(access_condition)
                .filter(#module::Column::#relation_column.eq(parent)).lock_exclusive()
                .one(context).await?.ok_or(ApiError::NotFound)?;
            if !state.extensions.#policy().can_read(context, &model).await? { return Err(ApiError::NotFound); }
            if model.revision != revision { return Err(ApiError::ConcurrentModification); }
            Ok(model)
        }
        pub async fn create(state: &AppState, context: &RequestContext<'_>, parent: uuid::Uuid, value: serde_json::Value) -> Result<#module::Model, ApiError> {
            let mut input: CreateInput = decode(value, parent, true)?;
            authorize_create_fields(context, &input)?;
            state.extensions.#hooks().before_validate_create(context, &mut input).await?;
            validate_create(&input)?;
            state.extensions.#hooks().before_create(context, &mut input).await?;
            authorize_create_fields(context, &input)?;
            validate_create(&input)?;
            if !(#create_allowed) { return Err(access_denied(context)); }
            if !state.extensions.#policy().can_create(context, &input).await? { return Err(ApiError::Forbidden); }
            let active = #module::ActiveModel { #(#creates,)* };
            let candidate = active.clone().try_into_model()?;
            if candidate.#relation_name != parent { return Err(invalid("Parent relation cannot change")); }
            validate_relations(state, context, &candidate).await?;
            let model = active.insert(context).await?;
            state.extensions.#hooks().after_create(context, &model).await?;
            #create_event
            Ok(model)
        }
        pub async fn update(state: &AppState, context: &RequestContext<'_>, parent: uuid::Uuid, row: UpdateRow) -> Result<#module::Model, ApiError> {
            let mut input: UpdateInput = decode(row.input, parent, false)?;
            authorize_update_fields(context, &input)?;
            state.extensions.#hooks().before_validate_update(context, &mut input).await?;
            validate_update(&input)?;
            let before = load(state, context, parent, row.id, row.revision).await?;
            state.extensions.#hooks().before_update(context, &before, &mut input).await?;
            authorize_update_fields(context, &input)?;
            validate_update(&input)?;
            let mut active = before.clone().into_active_model();
            #(#updates)*
            active.revision = Set(before.revision.checked_add(1).ok_or(ApiError::Internal)?);
            let candidate = active.clone().try_into_model()?;
            if candidate.#relation_name != parent { return Err(invalid("Parent relation cannot change")); }
            if !(#update_allowed) { return Err(access_denied(context)); }
            if !state.extensions.#policy().can_update(context, &before, &input, &candidate).await? { return Err(ApiError::Forbidden); }
            validate_relations(state, context, &candidate).await?;
            let after = active.update(context).await?;
            state.extensions.#hooks().after_update(context, &before, &after).await?;
            #update_event
            Ok(after)
        }
        pub async fn delete(state: &AppState, context: &RequestContext<'_>, parent: uuid::Uuid, row: DeleteRow) -> Result<#module::Model, ApiError> {
            let model = load(state, context, parent, row.id, row.revision).await?;
            if !(#delete_allowed) { return Err(access_denied(context)); }
            if !state.extensions.#policy().can_delete(context, &model).await? { return Err(ApiError::Forbidden); }
            state.extensions.#hooks().before_delete(context, &model).await?;
            model.clone().delete(context).await?;
            state.extensions.#hooks().after_delete(context, &model).await?;
            #delete_event
            Ok(model)
        }
        pub async fn committed(state: &AppState, context: &RequestContext<'_>, operation: crate::HookOperation, model: &#module::Model) {
            let suffix = match operation { crate::HookOperation::Create => "created", crate::HookOperation::Delete => "deleted", _ => "updated" };
            publish_realtime_event(state, context, &format!("{}.{}", #event_prefix, suffix), model);
            run_after_commit(state, operation, model, context.actor().cloned(), context.tenant()).await;
        }
    })
}

fn events(
    ir: &AppIr,
    entity: &EntityIr,
    primary: &syn::Ident,
    operation: &str,
    model: &TokenStream,
    before: Option<&TokenStream>,
) -> TokenStream {
    let entity_id = &entity.id.0;
    let deleted = operation == "delete";
    let before = before.map_or_else(
        || {
            if deleted {
                quote! { Some(&#model) }
            } else {
                quote! { None }
            }
        },
        |before| quote! { Some(&#before) },
    );
    let after = if deleted {
        quote! { None }
    } else {
        quote! { Some(&#model) }
    };
    let audit = entity.audit_enabled.then(|| quote! {
        crate::audit::record(context, context, #entity_id, #model.#primary.to_string(), #operation, #before, #after).await?;
    });
    let activity = ir.activity.resource_for_entity(&entity.id).map(|resource| {
        let resource = &resource.resource;
        let event = format!("{operation}d");
        quote! { crate::activity::record_system_event(context, context, #resource, #model.#primary.to_string(), #event).await?; }
    });
    quote! { #audit #activity }
}
