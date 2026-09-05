use super::relation;
use super::{column_ident, filter_keys, filter_rules, primary_key, search_rule};
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
    let mut keys = filter_keys(entity);
    keys.extend(relation::filter_keys(ir, entity)?);
    let filter_validation = super::filter_validation(&keys);
    let filters = filter_rules(entity, module)?;
    let relation_filters = relation::filter_rules(ir, entity, module)?;
    let search = search_rule(entity, module)?;
    let access_scope = super::access::scope(entity, module, &entity.access.list)?;
    let metric_arms = metric_arms(entity, module)?;
    let group_arms = group_arms(entity, module)?;
    let expression_trait = entity
        .fields
        .iter()
        .any(|field| {
            field.capabilities.filterable
                && matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal)
        })
        .then(|| quote! { use sea_orm::ExprTrait as _; });
    let handler = aggregate_handler(&AggregateHandlerTokens {
        module,
        policy,
        filter_validation: &filter_validation,
        access_scope: &access_scope,
        filters: &filters,
        relation_filters: &relation_filters,
        search: &search,
        metric_arms: &metric_arms,
        group_arms: &group_arms,
    });
    Ok(quote! {
        use std::collections::BTreeSet;
        #expression_trait

        #[derive(Debug, Default, Deserialize)]
        pub struct AggregateQuery {
            metrics: Option<String>,
            group_by: Option<String>,
            limit: Option<u64>,
            q: Option<String>,
            #[serde(flatten)]
            filters: BTreeMap<String, String>,
        }

        #[derive(Debug, Serialize)]
        pub struct AggregateMeta {
            metrics: Vec<String>,
            group_by: Vec<String>,
            limit: u64,
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
    module: &'a syn::Ident,
    policy: &'a syn::Ident,
    filter_validation: &'a TokenStream,
    access_scope: &'a TokenStream,
    filters: &'a [TokenStream],
    relation_filters: &'a [TokenStream],
    search: &'a TokenStream,
    metric_arms: &'a [TokenStream],
    group_arms: &'a [TokenStream],
}

fn aggregate_handler(tokens: &AggregateHandlerTokens<'_>) -> TokenStream {
    let AggregateHandlerTokens {
        module,
        policy,
        filter_validation,
        access_scope,
        filters,
        relation_filters,
        search,
        metric_arms,
        group_arms,
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
            #filter_validation
            let limit = query.limit.unwrap_or(100);
            if !(1..=500).contains(&limit) {
                return Err(ApiError::InvalidQuery(
                    "`limit` must be between 1 and 500".to_owned()
                ));
            }
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
            let mut select = #module::Entity::find();
            #access_scope
            #(#filters)*
            #(#relation_filters)*
            #search
            let mut select = select.select_only();
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
            for field in &group_by {
                if !selected_groups.insert(*field) {
                    return Err(ApiError::InvalidQuery(format!("duplicate group field `{field}`")));
                }
                match *field {
                    #(#group_arms,)*
                    _ => return Err(ApiError::InvalidQuery(format!("group field `{field}` is not allowed"))),
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

fn group_arms(entity: &EntityIr, module: &syn::Ident) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable && groupable(&field.ty))
        .map(|field| {
            let name = LitStr::new(&field.rust_name, Span::call_site());
            let column = column_ident(field)?;
            let alias = LitStr::new(&format!("group_{}", field.rust_name), Span::call_site());
            let selected = if matches!(field.ty, FieldTypeIr::Bigint | FieldTypeIr::Decimal) {
                quote! { sea_orm::sea_query::Expr::col((#module::Entity, #module::Column::#column)).cast_as("text") }
            } else {
                quote! { #module::Column::#column }
            };
            Ok(quote! {
                #name => {
                    if !field_read_allowed(&context, #name) {
                        return Err(access_denied(&context));
                    }
                    select = select
                        .column_as(#selected, #alias)
                        .group_by(#module::Column::#column);
                }
            })
        })
        .collect()
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
