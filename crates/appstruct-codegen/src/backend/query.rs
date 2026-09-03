pub(super) mod aggregate;
mod filters;
pub(super) mod helpers;
mod relation;
use super::access;
use super::parse_ident;
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr};
use filters::{
    filter_keys, filter_rules, filter_validation, parsed_value, primary_key, search_rule,
    sort_rules, supports_range,
};
use helpers::column_ident;
use proc_macro2::{Ident, TokenStream};
use quote::quote;
pub(super) fn list_support(
    ir: &AppIr,
    entity: &EntityIr,
    module: &syn::Ident,
    policy: &Ident,
) -> Result<TokenStream, CodegenError> {
    let filters = filter_rules(entity, module)?;
    let relation_filters = relation::filter_rules(ir, entity, module)?;
    let mut filter_keys = filter_keys(entity);
    filter_keys.extend(relation::filter_keys(ir, entity)?);
    let filter_validation = filter_validation(&filter_keys);
    let search = search_rule(entity, module)?;
    let sorts = sort_rules(entity, module)?;
    let column_trait = (!filters.is_empty()
        || !relation_filters.is_empty()
        || !sorts.is_empty()
        || entity.views.soft_delete)
        .then(|| quote! { ColumnTrait as _, });
    let primary_field = primary_key(entity)?;
    let primary = column_ident(primary_field)?;
    let cursor_value = parsed_value(primary_field, &quote! { raw_cursor.as_str() });
    let primary_name = parse_ident(&primary_field.rust_name)?;
    let access_scope = access::scope(entity, module, &entity.access.list)?;
    let query_trait = relation::has_filters(ir, entity)?.then(|| quote! { QueryTrait as _, });
    let handler = list_handler(&ListHandlerTokens {
        module,
        policy,
        filter_validation: &filter_validation,
        access_scope: &access_scope,
        filters: &filters,
        relation_filters: &relation_filters,
        search: &search,
        cursor_value: &cursor_value,
        primary: &primary,
        primary_name: &primary_name,
        sorts: &sorts,
    });
    Ok(quote! {
        use appstruct_runtime::{ListMeta, ListQuery, ListResponse, decode_cursor, encode_cursor};
        use sea_orm::{#column_trait #query_trait Condition, PaginatorTrait, QueryFilter, QueryOrder};
        #handler
    })
}
struct ListHandlerTokens<'a> {
    module: &'a syn::Ident,
    policy: &'a Ident,
    filter_validation: &'a TokenStream,
    access_scope: &'a TokenStream,
    filters: &'a [TokenStream],
    relation_filters: &'a [TokenStream],
    search: &'a TokenStream,
    cursor_value: &'a TokenStream,
    primary: &'a syn::Ident,
    primary_name: &'a syn::Ident,
    sorts: &'a [TokenStream],
}
#[allow(clippy::too_many_lines)]
fn list_handler(tokens: &ListHandlerTokens<'_>) -> TokenStream {
    let ListHandlerTokens {
        module,
        policy,
        filter_validation,
        access_scope,
        filters,
        relation_filters,
        search,
        cursor_value,
        primary,
        primary_name,
        sorts,
    } = tokens;
    quote! {
        async fn list(
            State(state): State<AppState>,
            headers: HeaderMap,
            axum::extract::Query(query): axum::extract::Query<ListQuery>,
        ) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? { return Err(access_denied(&context)); }
            #filter_validation
            let mut select = #module::Entity::find();
            #access_scope
            #(#filters)*
            #(#relation_filters)*
            #search
            let cursor_mode = query.cursor.is_some() || query.limit.is_some();
            if cursor_mode {
                if query.page.is_some() || query.page_size.is_some() || query.sort.is_some() {
                    return Err(ApiError::InvalidQuery(
                        "cursor pagination cannot be combined with `page`, `page_size`, or `sort`".to_owned()
                    ));
                }
                let limit = query.limit.unwrap_or(25);
                if !(1..=100).contains(&limit) {
                    return Err(ApiError::InvalidQuery(
                        "`limit` must be between 1 and 100".to_owned()
                    ));
                }
                if let Some(cursor) = query.cursor.as_deref() {
                    let raw_cursor = decode_cursor(cursor)
                        .ok_or_else(|| ApiError::InvalidQuery("invalid cursor".to_owned()))?;
                    let cursor_value = #cursor_value;
                    select = select.filter(#module::Column::#primary.gt(cursor_value));
                }
                let mut data = select
                    .order_by_asc(#module::Column::#primary)
                    .limit(limit + 1)
                    .all(&state.database)
                    .await?;
                let has_more = data.len() > usize::try_from(limit).unwrap_or(usize::MAX);
                if has_more {
                    data.pop();
                }
                let next_cursor = has_more.then(|| {
                    data.last()
                        .map(|model| encode_cursor(&model.#primary_name.to_string()))
                        .expect("a cursor page with more rows is not empty")
                });
                let data = data
                    .into_iter()
                    .map(|model| redact_model(&context, model))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Json(ListResponse {
                    data,
                    meta: ListMeta::Cursor { limit, next_cursor, has_more },
                }));
            }
            let page = query.page.unwrap_or(1);
            let page_size = query.page_size.unwrap_or(25);
            if page == 0 || !(1..=100).contains(&page_size) {
                return Err(ApiError::InvalidQuery(
                    "`page` must be at least 1 and `page_size` must be between 1 and 100".to_owned()
                ));
            }
            let mut primary_sorted = false;
            if let Some(sort) = &query.sort {
                for item in sort.split(',').filter(|item| !item.is_empty()) {
                    let (descending, name) = item.strip_prefix('-').map_or((false, item), |name| (true, name));
                    match name {
                        #(#sorts,)*
                        _ => return Err(ApiError::InvalidQuery(format!("field `{name}` is not sortable"))),
                    }
                }
            }
            if !primary_sorted {
                select = select.order_by_asc(#module::Column::#primary);
            }
            let total = select.clone().count(&state.database).await?;
            let data = select.paginate(&state.database, page_size).fetch_page(page - 1).await?
                .into_iter()
                .map(|model| redact_model(&context, model))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Json(ListResponse {
                data,
                meta: ListMeta::Page { page, page_size, total },
            }))
        }
    }
}
