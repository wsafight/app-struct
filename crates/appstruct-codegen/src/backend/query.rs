pub(super) mod aggregate;
pub(super) mod facets;
pub(super) mod filters;
pub(super) mod helpers;
mod relation;
use super::access;
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr};
use filters::{
    filter_keys, filter_rules, filter_validation, parsed_value, primary_key, search_rule,
    supports_range,
};
use helpers::column_ident;
use proc_macro2::{Ident, TokenStream};
use quote::quote;

#[allow(clippy::too_many_lines)]
pub(super) fn list_support(
    ir: &AppIr,
    entity: &EntityIr,
    module: &syn::Ident,
    policy: &Ident,
) -> Result<TokenStream, CodegenError> {
    let filters = filter_rules(entity, module)?;
    let relation_filters = relation::filter_rules(ir, entity, module)?;
    let mut keys = filter_keys(entity);
    keys.extend(relation::filter_keys(ir, entity)?);
    let validation = filter_validation(&keys);
    let search = search_rule(entity, module)?;
    let scope = access::scope(entity, module, &entity.access.list)?;
    let names = entity
        .fields
        .iter()
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>();
    Ok(quote! {
        use appstruct_runtime::{ListMeta, ListQuery, ListResponse, MAX_LIST_PAGE, list_page_is_valid};
        use sea_query::Condition;
        use crate::database::DatabaseExecutor as _;

        fn filtered_select(context: &RequestContext, query: &ListQuery) -> Result<crate::database::Select<#module::Model>, ApiError> {
            #validation
            let mut select = #module::select();
            #scope
            #(#filters)*
            #(#relation_filters)*
            #search
            if let Some(raw) = query.expression.as_deref() {
                let expression = appstruct_runtime::FilterExpression::parse(raw).map_err(ApiError::InvalidQuery)?;
                select = select.filter(crate::database::filter_expression::<#module::Model>(&expression, &|field| field_read_allowed(context, field))?);
            }
            Ok(select)
        }

        fn selected_fields(context: &RequestContext, raw: Option<&str>) -> Result<Vec<String>, ApiError> {
            let all = [#(#names),*];
            match raw {
                None => Ok(all.into_iter().filter(|field| field_read_allowed(context, field)).map(str::to_owned).collect()),
                Some(raw) if raw.is_empty() || raw.len() > 4096 => Err(ApiError::InvalidQuery("invalid fields projection".to_owned())),
                Some(raw) => {
                    let mut fields = Vec::new();
                    for name in raw.split(',') {
                        if !all.contains(&name) { return Err(ApiError::InvalidQuery(format!("unknown projected field `{name}`"))); }
                        if !field_read_allowed(context, name) { return Err(access_denied(context)); }
                        if fields.iter().any(|field| field == name) { return Err(ApiError::InvalidQuery("duplicate projected field".to_owned())); }
                        fields.push(name.to_owned());
                    }
                    Ok(fields)
                }
            }
        }

        async fn list(
            State(state): State<AppState>, headers: HeaderMap,
            axum::extract::Query(query): axum::extract::Query<ListQuery>,
        ) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
            use sift_query::SearchEntity as _;
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? { return Err(access_denied(&context)); }
            let mut select = filtered_select(&context, &query)?;
            let fields = selected_fields(&context, query.fields.as_deref())?;
            let order = crate::database::resource_order::<#module::Model>(query.sort.as_deref(), |name| field_read_allowed(&context, name))?;
            let cursor_mode = query.cursor.is_some() || query.limit.is_some() || query.direction.is_some();
            if cursor_mode {
                if query.page.is_some() || query.page_size.is_some() {
                    return Err(ApiError::InvalidQuery("cursor pagination cannot be combined with `page` or `page_size`".to_owned()));
                }
                if order.iter().any(|item| #module::Model::META.field(item.field).is_none_or(|field| !field.value_type.supports_cursor())) {
                    return Err(ApiError::InvalidQuery("sort contains a field that cannot be used in cursor pagination".to_owned()));
                }
                let limit = query.limit.unwrap_or(25);
                if !(1..=100).contains(&limit) { return Err(ApiError::InvalidQuery("`limit` must be between 1 and 100".to_owned())); }
                let previous = match query.direction.as_deref() {
                    None | Some("next") => false,
                    Some("previous") if query.cursor.is_some() => true,
                    Some("previous") => return Err(ApiError::InvalidQuery("`direction=previous` requires a cursor".to_owned())),
                    Some(_) => return Err(ApiError::InvalidQuery("`direction` must be `next` or `previous`".to_owned())),
                };
                let has_cursor = query.cursor.is_some();
                if let Some(token) = query.cursor.as_deref() {
                    if token.len() > crate::database::MAX_CURSOR_BYTES { return Err(ApiError::InvalidQuery("cursor is too long".to_owned())); }
                    let cursor = state.database.cursor_codec().decode::<#module::Model>(token, &order)
                        .map_err(|error| ApiError::InvalidQuery(error.to_string()))?;
                    select = select.filter(crate::database::cursor_condition::<#module::Model>(&order, cursor.values(), previous)?);
                }
                let page = select.order_by(&order, &#module::Model::META, previous).limit(limit + 1);
                let mut data = #module::projected(page, &fields, &order)?.all(&state.database).await?;
                let has_extra = data.len() > usize::try_from(limit).unwrap_or(usize::MAX);
                if has_extra { data.pop(); }
                if previous { data.reverse(); }
                let has_previous = if previous { has_extra } else { has_cursor };
                let has_next = if previous { has_cursor } else { has_extra };
                let encode = |row: &#module::ProjectedModel| -> Result<String, ApiError> {
                    let token = state.database.cursor_codec().encode::<#module::Model>(&order, &row.cursor_values(&order)?)
                        .map_err(|error| ApiError::InvalidQuery(error.to_string()))?;
                    if token.len() > crate::database::MAX_CURSOR_BYTES {
                        return Err(ApiError::InvalidQuery("sort values exceed the cursor size limit; use page pagination".to_owned()));
                    }
                    Ok(token)
                };
                let previous_cursor = data.first().filter(|_| has_previous).map(encode).transpose()?;
                let next_cursor = data.last().filter(|_| has_next).map(encode).transpose()?;
                let data = data.into_iter().map(|row| row.json(&fields)).collect::<Result<Vec<_>, _>>()?;
                return Ok(Json(ListResponse { data, meta: ListMeta::Cursor { limit, next_cursor, previous_cursor, has_more: has_next } }));
            }
            let page = query.page.unwrap_or(1);
            let page_size = query.page_size.unwrap_or(25);
            if !list_page_is_valid(page, page_size) {
                return Err(ApiError::InvalidQuery(format!("`page` must be between 1 and {MAX_LIST_PAGE} and `page_size` must be between 1 and 100")));
            }
            let total = select.clone().count(&state.database).await?;
            let page_select = select.order_by(&order, &#module::Model::META, false).limit(page_size).offset((page - 1) * page_size);
            let data = #module::projected(page_select, &fields, &order)?.all(&state.database).await?
                .into_iter().map(|row| row.json(&fields)).collect::<Result<Vec<_>, _>>()?;
            Ok(Json(ListResponse { data, meta: ListMeta::Page { page, page_size, total } }))
        }
    })
}
