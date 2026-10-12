use super::BulkContext;
use appstruct_ir::{EntityIr, FieldTypeIr};
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn export(_entity: &EntityIr, context: &BulkContext<'_>) -> TokenStream {
    let BulkContext { module, policy, .. } = context;
    quote! {
        async fn export_csv(
            State(state): State<AppState>, headers: HeaderMap,
            axum::extract::Query(query): axum::extract::Query<ListQuery>,
        ) -> Result<axum::response::Response, ApiError> {
            use axum::response::IntoResponse as _;
            use sift_query::SearchEntity as _;
            use futures_util::TryStreamExt as _;
            if query.page.is_some() || query.page_size.is_some() || query.cursor.is_some() || query.limit.is_some() || query.direction.is_some() {
                return Err(ApiError::InvalidQuery("CSV exports the complete filtered result; pagination is not accepted".to_owned()));
            }
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? { return Err(access_denied(&context)); }
            let select = filtered_select(&context, &query)?;
            let fields = selected_fields(&context, query.fields.as_deref())?;
            let order = crate::database::resource_order::<#module::Model>(query.sort.as_deref(), |field| field_read_allowed(&context, field))?;
            let transaction = state.database.begin().await?;
            transaction.execute_unprepared("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").await?;
            transaction.execute_unprepared("SET LOCAL statement_timeout = '30s'; SET LOCAL idle_in_transaction_session_timeout = '30s'").await?;
            if select.bounded_count(&transaction, MAX_CSV_EXPORT_ROWS).await? > MAX_CSV_EXPORT_ROWS {
                return Err(ApiError::InvalidQuery(format!("CSV export is limited to {MAX_CSV_EXPORT_ROWS} rows")));
            }
            let statement = #module::projected(select.order_by(&order, &#module::Model::META, false).limit(MAX_CSV_EXPORT_ROWS), &fields, &order)?.statement();
            let stream: std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Vec<u8>, std::io::Error>> + Send>> = Box::pin(async_stream::try_stream! {
                let mut rows = std::pin::pin!(transaction.stream_models::<#module::ProjectedModel>(statement));
                let mut chunk = fields.iter().map(|field| csv_escape(field)).collect::<Vec<_>>().join(",");
                chunk.push('\n');
                yield std::mem::take(&mut chunk).into_bytes();
                while let Some(row) = rows.try_next().await.map_err(std::io::Error::other)? {
                    let value = row.json(&fields).map_err(|_| std::io::Error::other("CSV serialization failed"))?;
                    let object = value.as_object().ok_or_else(|| std::io::Error::other("invalid CSV row"))?;
                    chunk.push_str(&fields.iter().map(|field| csv_escape(&object.get(field).map(csv_cell).unwrap_or_default())).collect::<Vec<_>>().join(","));
                    chunk.push('\n');
                    if chunk.len() >= 65_536 { yield std::mem::take(&mut chunk).into_bytes(); }
                }
                if !chunk.is_empty() { yield chunk.into_bytes(); }
            });
            let body = axum::body::Body::from_stream(stream);
            Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=export.csv")], body).into_response())
        }
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn import(entity: &EntityIr, context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        hooks,
        policy,
        create_allowed,
        create_values,
        active_default,
        primary,
        entity_id,
        audit_enabled,
        activity_resource,
        ..
    } = context;
    let fields = entity
        .fields
        .iter()
        .filter(|field| field.generated.is_none() && !entity.is_workflow_field(field))
        .collect::<Vec<_>>();
    let field_names = fields
        .iter()
        .map(|field| field.api_name.as_str())
        .collect::<Vec<_>>();
    let field_match = fields.iter().map(|field| {
        let name = field.api_name.as_str();
        let kind = kind(&field.ty);
        if matches!(field.ty, FieldTypeIr::Json) {
            quote! { #name => {
                let value = if raw.is_empty() { serde_json::Value::Null } else {
                    match serde_json::from_str(raw) {
                        Ok(value) => value,
                        Err(_) => { row_error = Some(format!("CSV column `{}` must contain valid JSON", #name)); break; }
                    }
                };
                object.insert(#name.to_owned(), value);
            } }
        } else {
            quote! { #name => { object.insert(#name.to_owned(), csv_json_value(raw, #kind)); } }
        }
    });
    let audit = super::audit_event(*audit_enabled, entity_id, primary, "create");
    let activity = super::activity_event(*activity_resource, primary, "created");
    let create_event = format!("{module}.created");
    quote! {
        async fn import_csv(State(state): State<AppState>, headers: HeaderMap, body: String) -> Result<Json<BulkResult>, ApiError> {
            let context = state.mutation_context(&headers).await?;
            let rows = parse_csv_rows(&body)
                .map_err(|error| ApiError::InvalidQuery(error.to_string()))?;
            let Some(header_row) = rows.first() else { return Ok(Json(BulkResult { succeeded: Vec::new(), failed: Vec::new() })); };
            if rows.len() - 1 > MAX_CSV_IMPORT_ROWS {
                return Err(ApiError::InvalidQuery(format!("CSV import is limited to {MAX_CSV_IMPORT_ROWS} rows")));
            }
            let expected = [#(#field_names),*];
            let unique = header_row.iter().collect::<std::collections::BTreeSet<_>>();
            if header_row.iter().any(String::is_empty) || unique.len() != header_row.len() {
                return Err(ApiError::InvalidQuery("CSV headers must be nonempty and unique".to_owned()));
            }
            if header_row.iter().map(String::as_str).any(|name| !expected.contains(&name)) { return Err(ApiError::InvalidQuery("CSV contains an unknown column".to_owned())); }
            let actor = context.actor().cloned(); let tenant = context.tenant(); let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            let mut created = Vec::new();
            for (index, row) in rows.iter().skip(1).enumerate() {
                let row_id = index.to_string();
                if row.len() != header_row.len() {
                    result.failed.push(bulk_failure(&row_id, "invalid_row", "CSV row has a different number of columns than the header"));
                    continue;
                }
                let mut object = serde_json::Map::new();
                #[allow(unused_mut)]
                let mut row_error: Option<String> = None;
                for (column, raw) in header_row.iter().zip(row.iter()) { match column.as_str() { #(#field_match)* _ => {} } }
                if let Some(error) = row_error { result.failed.push(bulk_failure(&row_id, "invalid_row", error)); continue; }
                let mut input: CreateInput = match serde_json::from_value(serde_json::Value::Object(object)) { Ok(input) => input, Err(error) => { result.failed.push(bulk_failure(&row_id, "invalid_row", error.to_string())); continue; } };
                let preparation: Result<(), ApiError> = async {
                    authorize_create_fields(&context, &input)?;
                    state.extensions.#hooks().before_validate_create(&context, &mut input).await?;
                    validate_create(&input)?;
                    Ok(())
                }.await;
                if let Err(error) = preparation {
                    result.failed.push(error.into_bulk_failure(&row_id));
                    continue;
                }
                let savepoint = transaction.begin().await?;
                let outcome: Result<#module::Model, ApiError> = async {
                    let context = RequestContext::transaction_with_file(&savepoint, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                    state.extensions.#hooks().before_create(&context, &mut input).await?;
                    authorize_create_fields(&context, &input)?;
                    validate_create(&input)?;
                    if !(#create_allowed) {
                        return Err(access_denied(&context));
                    }
                    if !state.extensions.#policy().can_create(&context, &input).await? {
                        return Err(ApiError::Forbidden);
                    }
                    let active = #module::Model { #(#create_values,)* #active_default };
                    let model = active.insert(&savepoint).await?;
                    state.extensions.#hooks().after_create(&context, &model).await?;
                    #audit
                    #activity
                    Ok(model)
                }.await;
                match outcome {
                    Ok(model) => {
                        savepoint.commit().await?;
                        result.succeeded.push(row_id);
                        created.push(model);
                    }
                    Err(error) => {
                        savepoint.rollback().await?;
                        result.failed.push(error.into_bulk_failure(&row_id));
                    }
                }
            }
            transaction.commit().await?;
            for model in &created {
                publish_realtime_event(&state, &context, #create_event, model);
                run_after_commit(&state, crate::HookOperation::Create, model, actor.clone(), tenant).await;
            }
            Ok(Json(result))
        }
    }
}

fn kind(ty: &FieldTypeIr) -> &'static str {
    match ty {
        FieldTypeIr::Boolean => "boolean",
        FieldTypeIr::Integer => "integer",
        _ => "string",
    }
}
