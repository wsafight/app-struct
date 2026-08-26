use super::{parse_ident, rust_type};
use crate::CodegenError;
use appstruct_ir::{EntityIr, FieldIr, FieldTypeIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(super) fn list_support(
    entity: &EntityIr,
    module: &syn::Ident,
) -> Result<TokenStream, CodegenError> {
    let filters = filter_rules(entity, module)?;
    let filter_keys = filter_keys(entity);
    let search = search_rule(entity, module)?;
    let sorts = sort_rules(entity, module)?;
    let primary = column_ident(primary_key(entity)?)?;
    Ok(quote! {
        use sea_orm::{ColumnTrait, Condition, PaginatorTrait, QueryFilter, QueryOrder};

        #[derive(Debug, Default, Deserialize)]
        pub struct ListQuery {
            page: Option<u64>,
            page_size: Option<u64>,
            sort: Option<String>,
            q: Option<String>,
            #[serde(flatten)]
            filters: BTreeMap<String, String>,
        }

        #[derive(Debug, Serialize)]
        pub struct ListMeta {
            page: u64,
            page_size: u64,
            total: u64,
        }

        #[derive(Debug, Serialize)]
        pub struct ListResponse<T> {
            data: Vec<T>,
            meta: ListMeta,
        }

        async fn list(
            State(state): State<AppState>,
            axum::extract::Query(query): axum::extract::Query<ListQuery>,
        ) -> Result<Json<ListResponse<#module::Model>>, ApiError> {
            let page = query.page.unwrap_or(1);
            let page_size = query.page_size.unwrap_or(25);
            if page == 0 || !(1..=100).contains(&page_size) {
                return Err(ApiError::InvalidQuery(
                    "`page` must be at least 1 and `page_size` must be between 1 and 100".to_owned()
                ));
            }
            for key in query.filters.keys() {
                match key.as_str() {
                    #(#filter_keys => {},)*
                    _ => return Err(ApiError::InvalidQuery(format!("unsupported query parameter `{key}`"))),
                }
            }
            let mut select = #module::Entity::find();
            #(#filters)*
            #search
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
            let data = select.paginate(&state.database, page_size).fetch_page(page - 1).await?;
            Ok(Json(ListResponse { data, meta: ListMeta { page, page_size, total } }))
        }
    })
}

fn filter_rules(entity: &EntityIr, module: &syn::Ident) -> Result<Vec<TokenStream>, CodegenError> {
    let mut rules = Vec::new();
    for field in entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
    {
        let column = column_ident(field)?;
        let exact_key = LitStr::new(&format!("filter[{}]", field.rust_name), Span::call_site());
        let value = parsed_value(field, &quote! { raw });
        rules.push(quote! {
            if let Some(raw) = query.filters.get(#exact_key) {
                let value = #value;
                select = select.filter(#module::Column::#column.eq(value));
            }
        });
        if supports_range(&field.ty) {
            for (operator, method) in [("gte", "gte"), ("lte", "lte")] {
                let key = LitStr::new(
                    &format!("filter[{}][{operator}]", field.rust_name),
                    Span::call_site(),
                );
                let method = parse_ident(method)?;
                let value = parsed_value(field, &quote! { raw });
                rules.push(quote! {
                    if let Some(raw) = query.filters.get(#key) {
                        let value = #value;
                    select = select.filter(#module::Column::#column.#method(value));
                    }
                });
            }
        }
    }
    Ok(rules)
}

fn filter_keys(entity: &EntityIr) -> Vec<LitStr> {
    let mut keys = Vec::new();
    for field in entity
        .fields
        .iter()
        .filter(|field| field.capabilities.filterable)
    {
        keys.push(LitStr::new(
            &format!("filter[{}]", field.rust_name),
            Span::call_site(),
        ));
        if supports_range(&field.ty) {
            keys.push(LitStr::new(
                &format!("filter[{}][gte]", field.rust_name),
                Span::call_site(),
            ));
            keys.push(LitStr::new(
                &format!("filter[{}][lte]", field.rust_name),
                Span::call_site(),
            ));
        }
    }
    keys
}

fn search_rule(entity: &EntityIr, module: &syn::Ident) -> Result<TokenStream, CodegenError> {
    let searchable = entity
        .fields
        .iter()
        .filter(|field| field.capabilities.searchable)
        .map(column_ident)
        .collect::<Result<Vec<_>, _>>()?;
    if searchable.is_empty() {
        return Ok(quote! {
            if query.q.as_ref().is_some_and(|value| !value.trim().is_empty()) {
                return Err(ApiError::InvalidQuery("this resource does not support search".to_owned()));
            }
        });
    }
    Ok(quote! {
        if let Some(term) = query.q.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            let mut condition = Condition::any();
            #(condition = condition.add(#module::Column::#searchable.contains(term));)*
            select = select.filter(condition);
        }
    })
}

fn sort_rules(entity: &EntityIr, module: &syn::Ident) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter(|field| field.primary_key || field.capabilities.sortable)
        .map(|field| {
            let name = LitStr::new(&field.rust_name, Span::call_site());
            let column = column_ident(field)?;
            let primary = field.primary_key;
            Ok(quote! {
                #name => {
                    select = if descending {
                        select.order_by_desc(#module::Column::#column)
                    } else {
                        select.order_by_asc(#module::Column::#column)
                    };
                    primary_sorted |= #primary;
                }
            })
        })
        .collect()
}

fn parsed_value(field: &FieldIr, raw: &TokenStream) -> TokenStream {
    let message = LitStr::new(
        &format!("invalid value for `{}`", field.rust_name),
        Span::call_site(),
    );
    let ty = rust_type(&field.ty);
    match field.ty {
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. } => {
            quote! { #raw.to_owned() }
        }
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => quote! {
            uuid::Uuid::parse_str(#raw).map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        FieldTypeIr::Date => quote! {
            chrono::NaiveDate::parse_from_str(#raw, "%Y-%m-%d")
                .map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        FieldTypeIr::Datetime => quote! {
            #raw.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        FieldTypeIr::Json => quote! {
            serde_json::from_str::<serde_json::Value>(#raw)
                .map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
        _ => quote! {
            #raw.parse::<#ty>().map_err(|_| ApiError::InvalidQuery(#message.to_owned()))?
        },
    }
}

fn supports_range(field_type: &FieldTypeIr) -> bool {
    matches!(
        field_type,
        FieldTypeIr::Integer
            | FieldTypeIr::Bigint
            | FieldTypeIr::Decimal
            | FieldTypeIr::Date
            | FieldTypeIr::Datetime
    )
}

fn primary_key(entity: &EntityIr) -> Result<&FieldIr, CodegenError> {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new(format!("entity `{}` has no primary key", entity.id)))
}

fn column_ident(field: &FieldIr) -> Result<syn::Ident, CodegenError> {
    let name = field
        .rust_name
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<String>();
    parse_ident(&name)
}
