use crate::CodegenError;
use appstruct_ir::{EntityIr, FieldTypeIr};
use proc_macro2::TokenStream;
use quote::quote;

pub(in crate::backend) fn support(
    entity: &EntityIr,
    module: &syn::Ident,
    policy: &syn::Ident,
) -> Result<TokenStream, CodegenError> {
    let arms = entity.fields.iter().filter(|field| field.capabilities.filterable && !matches!(field.ty, FieldTypeIr::Json)).map(|field| {
        let name = &field.rust_name;
        let column = super::helpers::column_ident(field)?;
        let template = if matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal) { "to_jsonb($1::text)" } else { "to_jsonb($1)" };
        Ok(quote! { #name => select.select_only()
            .column_as(sea_query::Expr::cust_with_exprs(#template, [sea_query::SimpleExpr::from(#module::Column::#column)]), "value")
            .column_as(sea_query::Expr::cust("count(*)"), "count")
            .group_by(#module::Column::#column)
            .order_by_desc(sea_query::Expr::cust("count(*)"))
            .order_by_asc(#module::Column::#column)
        })
    }).collect::<Result<Vec<_>, CodegenError>>()?;
    if arms.is_empty() {
        return Ok(quote! {
            async fn facets(State(state): State<AppState>, headers: HeaderMap, axum::extract::Query(_query): axum::extract::Query<ListQuery>) -> Result<Json<serde_json::Value>, ApiError> {
                let context = state.context(&headers).await?;
                if !state.extensions.#policy().can_list(&context).await? { return Err(access_denied(&context)); }
                Err(ApiError::InvalidQuery("this resource has no facet fields".to_owned()))
            }
        });
    }
    Ok(quote! {
        #[derive(Debug, Default, Deserialize)]
        struct FacetQuery {
            facet: Option<String>, limit: Option<u64>, q: Option<String>,
            #[serde(rename = "where")] expression: Option<String>,
            #[serde(flatten)] filters: BTreeMap<String, String>,
        }
        async fn facets(State(state): State<AppState>, headers: HeaderMap, axum::extract::Query(query): axum::extract::Query<FacetQuery>) -> Result<Json<serde_json::Value>, ApiError> {
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? { return Err(access_denied(&context)); }
            let names = query.facet.as_deref().unwrap_or("").split(',').collect::<Vec<_>>();
            let limit = query.limit.unwrap_or(10);
            if names.is_empty() || names.len() > 5 || names.iter().any(|name| name.is_empty()) || !(1..=20).contains(&limit) {
                return Err(ApiError::InvalidQuery("facets require 1 to 5 fields and limit between 1 and 20".to_owned()));
            }
            let filters = ListQuery { q: query.q, expression: query.expression, filters: query.filters, ..Default::default() };
            let select = filtered_select(&context, &filters)?;
            let transaction = state.database.begin().await?;
            transaction.execute_unprepared("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").await?;
            let total = select.count(&transaction).await?;
            let mut data = serde_json::Map::new();
            for name in names {
                if data.contains_key(name) { return Err(ApiError::InvalidQuery("duplicate facet field".to_owned())); }
                if !field_read_allowed(&context, name) { return Err(access_denied(&context)); }
                let select = select.clone();
                let facet: crate::database::Select<#module::Model> = match name { #(#arms,)* _ => return Err(ApiError::InvalidQuery(format!("field `{name}` cannot be used as a facet"))) };
                let buckets = facet.limit(limit).into_json().all(&transaction).await?;
                let included = buckets.iter().map(|bucket| bucket["count"].as_u64().unwrap_or(0)).sum::<u64>();
                data.insert(name.to_owned(), serde_json::json!({ "buckets": buckets, "other_count": total.saturating_sub(included) }));
            }
            transaction.rollback().await?;
            Ok(Json(serde_json::json!({ "data": data })))
        }
    })
}
