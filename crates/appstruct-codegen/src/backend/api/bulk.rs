use super::super::access;
use crate::CodegenError;
use appstruct_ir::{EntityIr, FieldIr, FieldTypeIr};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

mod csv;
mod mutations;

pub(super) struct SourceContext<'context> {
    pub module: &'context Ident,
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
    pub activity_resource: Option<&'context str>,
}

pub(super) struct BulkContext<'context> {
    pub module: &'context Ident,
    pub primary: &'context Ident,
    pub primary_column: &'context Ident,
    pub bulk_parse_id: &'context TokenStream,
    pub hooks: &'context Ident,
    pub policy: &'context Ident,
    pub list_scope: &'context TokenStream,
    pub read_scope: &'context TokenStream,
    pub delete_allowed: &'context TokenStream,
    pub update_allowed: &'context TokenStream,
    pub create_allowed: &'context TokenStream,
    pub create_values: &'context [TokenStream],
    pub active_default: Option<&'context TokenStream>,
    pub updates: &'context [TokenStream],
    pub entity_id: &'context str,
    pub audit_enabled: bool,
    pub soft_delete: bool,
    pub trash_scope: &'context TokenStream,
    pub restore_scope: &'context TokenStream,
    pub activity_resource: Option<&'context str>,
}

pub(super) fn source(
    entity: &EntityIr,
    source: &SourceContext<'_>,
) -> Result<TokenStream, CodegenError> {
    let SourceContext {
        module,
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
        activity_resource,
    } = source;
    let trash_scope = access::trash_scope(entity, module, &entity.access.list)?;
    let read_scope = access::scope(entity, module, &entity.access.read)?;
    let restore_scope = access::trash_scope(entity, module, &entity.access.read)?;
    let primary_field = entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new("entity must define a primary key"))?;
    let primary_column = super::super::query::helpers::column_ident(primary_field)?;
    let bulk_parse_id = bulk_parse_id_expression(primary_field);
    let context = BulkContext {
        module,
        primary,
        primary_column: &primary_column,
        bulk_parse_id: &bulk_parse_id,
        hooks,
        policy,
        list_scope,
        read_scope: &read_scope,
        delete_allowed,
        update_allowed,
        create_allowed,
        create_values,
        active_default: *active_default,
        updates,
        entity_id: &entity.id.0,
        audit_enabled: entity.audit_enabled,
        soft_delete: entity.views.soft_delete,
        trash_scope: &trash_scope,
        restore_scope: &restore_scope,
        activity_resource: *activity_resource,
    };
    let update = mutations::update(&context);
    let delete = mutations::delete(&context);
    let export = csv::export(entity, &context);
    let import = csv::import(entity, &context);
    let restore = context.soft_delete.then(|| mutations::restore(&context));
    let trash = context.soft_delete.then(|| trash_handler(&context));
    Ok(quote! {
        use appstruct_runtime::{
            BulkDeleteInput, BulkResult, BulkUpdateInput, CSV_EXPORT_PAGE_SIZE, MAX_BULK_ITEMS,
            MAX_CSV_EXPORT_ROWS, MAX_CSV_IMPORT_ROWS, bulk_failure, bulk_request_size_is_valid,
            csv_cell, csv_escape, csv_json_value, parse_csv_rows,
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

fn bulk_parse_id_expression(field: &FieldIr) -> TokenStream {
    match field.ty {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => {
            quote! { uuid::Uuid::parse_str(id_text).map_err(|_| ApiError::InvalidId) }
        }
        FieldTypeIr::Integer => {
            quote! { id_text.parse::<i32>().map_err(|_| ApiError::InvalidId) }
        }
        FieldTypeIr::Bigint => {
            quote! { id_text.parse::<i64>().map_err(|_| ApiError::InvalidId) }
        }
        _ => quote! { Ok::<String, ApiError>(id_text.to_owned()) },
    }
}

fn audit_event(enabled: bool, entity_id: &str, primary: &Ident, operation: &str) -> TokenStream {
    if !enabled {
        return TokenStream::new();
    }
    let entity_id = entity_id.to_owned();
    match operation {
        "create" => {
            quote! { crate::audit::record(&savepoint, &context, #entity_id, model.#primary.to_string(), "create", None, Some(&model)).await?; }
        }
        "update" => {
            quote! { crate::audit::record(&savepoint, &context, #entity_id, after.#primary.to_string(), "update", Some(&before), Some(&after)).await?; }
        }
        "delete" => {
            quote! { crate::audit::record(&savepoint, &context, #entity_id, deleted.#primary.to_string(), "delete", Some(&deleted), None).await?; }
        }
        "restore" => {
            quote! { crate::audit::record(&savepoint, &context, #entity_id, after.#primary.to_string(), "restore", Some(&before), Some(&after)).await?; }
        }
        _ => TokenStream::new(),
    }
}

fn activity_event(resource: Option<&str>, primary: &Ident, event: &str) -> TokenStream {
    let Some(resource) = resource else {
        return TokenStream::new();
    };
    let model = match event {
        "created" => quote! { model },
        "updated" | "restored" => quote! { after },
        "deleted" => quote! { deleted },
        _ => unreachable!(),
    };
    quote! {
        crate::activity::record_system_event(
            &savepoint, &context, #resource, #model.#primary.to_string(), #event,
        ).await?;
    }
}
