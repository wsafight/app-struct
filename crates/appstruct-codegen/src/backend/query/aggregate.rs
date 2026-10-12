mod group;

use super::{column_ident, primary_key};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub fn aggregate_support(
    ir: &AppIr,
    entity: &EntityIr,
    module: &syn::Ident,
    policy: &syn::Ident,
) -> Result<TokenStream, CodegenError> {
    let metric_arms = metric_arms(entity, module)?;
    let group_arms = group::arms(ir, entity, module)?;
    let metric_order_arms = metric_order_arms(entity, module)?;
    let group_order_arms = group::order_arms(ir, entity)?;
    let has_relation_groups = group::has_relation_groups(ir, entity)?;
    let time_fields = entity
        .fields
        .iter()
        .filter(|field| {
            field.capabilities.filterable
                && matches!(field.ty, FieldTypeIr::Date | FieldTypeIr::Datetime)
        })
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>();
    let relation_group_state = has_relation_groups.then(|| {
        quote! { let mut relation_group_selected = false; }
    });
    let handler = aggregate_handler(&AggregateHandlerTokens {
        policy,
        metric_arms: &metric_arms,
        group_arms: &group_arms,
        metric_order_arms: &metric_order_arms,
        group_order_arms: &group_order_arms,
        relation_group_state: relation_group_state.as_ref(),
        time_fields: &time_fields,
    });
    Ok(quote! {
        use std::collections::BTreeSet;

        #[derive(Debug, Default, Deserialize)]
        pub struct AggregateQuery {
            metrics: Option<String>,
            group_by: Option<String>,
            limit: Option<u64>,
            order: Option<String>,
            order_by: Option<String>,
            time_bucket: Option<String>,
            q: Option<String>,
            #[serde(rename = "where")]
            expression: Option<String>,
            #[serde(flatten)]
            filters: BTreeMap<String, String>,
        }

        #[derive(Debug, Serialize)]
        pub struct AggregateMeta {
            metrics: Vec<String>,
            group_by: Vec<String>,
            limit: u64,
            order: String,
            order_by: String,
            time_bucket: Option<String>,
        }

        #[derive(Debug, Serialize)]
        pub struct AggregateResponse {
            data: Vec<serde_json::Value>,
            meta: AggregateMeta,
        }

        #handler
    })
}

struct AggregateHandlerTokens<'a> {
    policy: &'a syn::Ident,
    metric_arms: &'a [TokenStream],
    group_arms: &'a [TokenStream],
    metric_order_arms: &'a [TokenStream],
    group_order_arms: &'a [TokenStream],
    relation_group_state: Option<&'a TokenStream>,
    time_fields: &'a [&'a str],
}

#[allow(clippy::too_many_lines)]
fn aggregate_handler(tokens: &AggregateHandlerTokens<'_>) -> TokenStream {
    let AggregateHandlerTokens {
        policy,
        metric_arms,
        group_arms,
        metric_order_arms,
        group_order_arms,
        relation_group_state,
        time_fields,
    } = tokens;
    quote! {
        async fn aggregate(
            State(state): State<AppState>,
            headers: HeaderMap,
            axum::extract::Query(query): axum::extract::Query<AggregateQuery>,
        ) -> Result<Json<AggregateResponse>, ApiError> {
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? {
                return Err(access_denied(&context));
            }
            let limit = query.limit.unwrap_or(100);
            if !(1..=500).contains(&limit) {
                return Err(ApiError::InvalidQuery(
                    "`limit` must be between 1 and 500".to_owned()
                ));
            }
            let time_bucket = query.time_bucket.as_deref();
            if time_bucket.is_some_and(|unit| !matches!(unit, "day" | "week" | "month")) {
                return Err(ApiError::InvalidQuery("`time_bucket` must be day, week or month".to_owned()));
            }
            let order_by = query.order_by.as_deref().unwrap_or(if time_bucket.is_some() { "group" } else { "metric" });
            if !matches!(order_by, "metric" | "group") {
                return Err(ApiError::InvalidQuery("`order_by` must be metric or group".to_owned()));
            }
            let order = query.order.as_deref().unwrap_or(if order_by == "group" { "asc" } else { "desc" });
            let descending = match order {
                "asc" => false,
                "desc" => true,
                _ => return Err(ApiError::InvalidQuery(
                    "`order` must be `asc` or `desc`".to_owned()
                )),
            };
            let metrics = query
                .metrics
                .as_deref()
                .unwrap_or("count")
                .split(',')
                .map(str::trim)
                .filter(|metric| !metric.is_empty())
                .collect::<Vec<_>>();
            if metrics.is_empty() {
                return Err(ApiError::InvalidQuery(
                    "at least one aggregate metric is required".to_owned()
                ));
            }
            let mut selected_metrics = BTreeSet::new();
            let resource_query = ListQuery { q: query.q.clone(), expression: query.expression.clone(), filters: query.filters.clone(), ..Default::default() };
            let mut select = filtered_select(&context, &resource_query)?.select_only();
            for metric in &metrics {
                if !selected_metrics.insert(*metric) {
                    return Err(ApiError::InvalidQuery(format!("duplicate aggregate metric `{metric}`")));
                }
                match *metric {
                    #(#metric_arms,)*
                    _ => return Err(ApiError::InvalidQuery(format!("aggregate metric `{metric}` is not allowed"))),
                }
            }
            let group_by = query
                .group_by
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .collect::<Vec<_>>();
            let mut selected_groups = BTreeSet::new();
            let time_fields: &[&str] = &[#(#time_fields),*];
            if time_bucket.is_some() && group_by.iter().filter(|field| time_fields.contains(field)).count() != 1 {
                return Err(ApiError::InvalidQuery("`time_bucket` requires exactly one local date or datetime group field".to_owned()));
            }
            if order_by == "group" && group_by.is_empty() {
                return Err(ApiError::InvalidQuery("`order_by=group` requires a group field".to_owned()));
            }
            #relation_group_state
            for field in &group_by {
                if !selected_groups.insert(*field) {
                    return Err(ApiError::InvalidQuery(format!("duplicate group field `{field}`")));
                }
                match *field {
                    #(#group_arms,)*
                    _ => return Err(ApiError::InvalidQuery(format!("group field `{field}` is not allowed"))),
                }
            }
            if order_by == "metric" {
                match metrics[0] {
                    #(#metric_order_arms,)*
                    _ => unreachable!("aggregate metrics were validated"),
                }
            }
            for field in &group_by {
                match *field {
                    #(#group_order_arms,)*
                    _ => unreachable!("aggregate groups were validated"),
                }
            }
            let data = select
                .limit(limit)
                .into_json()
                .all(&state.database)
                .await?;
            Ok(Json(AggregateResponse {
                data,
                meta: AggregateMeta {
                    metrics: metrics.into_iter().map(str::to_owned).collect(),
                    group_by: group_by.into_iter().map(str::to_owned).collect(),
                    limit,
                    order: order.to_owned(),
                    order_by: order_by.to_owned(),
                    time_bucket: time_bucket.map(str::to_owned),
                },
            }))
        }
    }
}

fn metric_arms(entity: &EntityIr, module: &syn::Ident) -> Result<Vec<TokenStream>, CodegenError> {
    let primary = column_ident(primary_key(entity)?)?;
    let mut arms = vec![quote! {
        "count" | "count:*" => {
            select = select.column_as(#module::Column::#primary.count(), "count");
        }
    }];
    for field in aggregate_fields(entity) {
        let column = column_ident(field)?;
        let field_name = LitStr::new(&field.rust_name, Span::call_site());
        let read_guard = quote! {
            if !field_read_allowed(&context, #field_name) {
                return Err(access_denied(&context));
            }
        };
        let sum_name = LitStr::new(&format!("sum:{}", field.rust_name), Span::call_site());
        let avg_name = LitStr::new(&format!("avg:{}", field.rust_name), Span::call_site());
        let min_name = LitStr::new(&format!("min:{}", field.rust_name), Span::call_site());
        let max_name = LitStr::new(&format!("max:{}", field.rust_name), Span::call_site());
        let sum_alias = LitStr::new(&format!("sum_{}", field.rust_name), Span::call_site());
        let avg_alias = LitStr::new(&format!("avg_{}", field.rust_name), Span::call_site());
        let min_alias = LitStr::new(&format!("min_{}", field.rust_name), Span::call_site());
        let max_alias = LitStr::new(&format!("max_{}", field.rust_name), Span::call_site());
        let wire_cast = if matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal) {
            quote! { .cast_as("text") }
        } else {
            TokenStream::new()
        };
        if supports_sum_avg(&field.ty) {
            arms.push(quote! {
                #sum_name => {
                    #read_guard
                    select = select.column_as(#module::Column::#column.sum() #wire_cast, #sum_alias);
                },
                #avg_name => {
                    #read_guard
                    select = select.column_as(#module::Column::#column.avg() #wire_cast, #avg_alias);
                }
            });
        }
        if supports_min_max(&field.ty) {
            arms.push(quote! {
                #min_name => {
                    #read_guard
                    select = select.column_as(#module::Column::#column.min() #wire_cast, #min_alias);
                },
                #max_name => {
                    #read_guard
                    select = select.column_as(#module::Column::#column.max() #wire_cast, #max_alias);
                }
            });
        }
    }
    Ok(arms)
}

fn metric_order_arms(
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<Vec<TokenStream>, CodegenError> {
    let primary = column_ident(primary_key(entity)?)?;
    let mut arms = vec![order_arm(
        &["count", "count:*"],
        &quote! { #module::Column::#primary.count() },
    )];
    for field in aggregate_fields(entity) {
        let column = column_ident(field)?;
        if supports_sum_avg(&field.ty) {
            arms.push(order_arm(
                &[&format!("sum:{}", field.rust_name)],
                &quote! { #module::Column::#column.sum() },
            ));
            arms.push(order_arm(
                &[&format!("avg:{}", field.rust_name)],
                &quote! { #module::Column::#column.avg() },
            ));
        }
        if supports_min_max(&field.ty) {
            arms.push(order_arm(
                &[&format!("min:{}", field.rust_name)],
                &quote! { #module::Column::#column.min() },
            ));
            arms.push(order_arm(
                &[&format!("max:{}", field.rust_name)],
                &quote! { #module::Column::#column.max() },
            ));
        }
    }
    Ok(arms)
}

fn order_arm(names: &[&str], expression: &TokenStream) -> TokenStream {
    let names = names
        .iter()
        .map(|name| LitStr::new(name, Span::call_site()));
    quote! {
        #(#names)|* => {
            select = if descending {
                select.order_by_desc(#expression)
            } else {
                select.order_by_asc(#expression)
            };
        }
    }
}

fn relation_group_alias(relation: &FieldIr, target: &FieldIr) -> String {
    format!("group_{}_{}", relation.api_name, target.rust_name)
}

fn aggregate_fields(entity: &EntityIr) -> impl Iterator<Item = &FieldIr> {
    entity.fields.iter().filter(|field| {
        field.capabilities.filterable
            && !field.primary_key
            && !matches!(field.ty, FieldTypeIr::Relation { .. } | FieldTypeIr::Json)
            && (supports_sum_avg(&field.ty) || supports_min_max(&field.ty))
    })
}

fn supports_sum_avg(field_type: &FieldTypeIr) -> bool {
    matches!(
        field_type,
        FieldTypeIr::Integer | FieldTypeIr::Bigint | FieldTypeIr::Decimal
    )
}

fn supports_min_max(field_type: &FieldTypeIr) -> bool {
    matches!(
        field_type,
        FieldTypeIr::Integer
            | FieldTypeIr::Bigint
            | FieldTypeIr::Decimal
            | FieldTypeIr::String
            | FieldTypeIr::Enum { .. }
            | FieldTypeIr::Date
            | FieldTypeIr::Datetime
    )
}

fn groupable(field_type: &FieldTypeIr) -> bool {
    !matches!(field_type, FieldTypeIr::Json | FieldTypeIr::Relation { .. })
}
