use super::BulkContext;
use appstruct_ir::{EntityIr, FieldTypeIr};
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn export(entity: &EntityIr, context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        policy,
        list_scope,
        primary_column,
        ..
    } = context;
    let headers = entity
        .fields
        .iter()
        .map(|field| field.api_name.as_str())
        .collect::<Vec<_>>();
    let fields = entity
        .fields
        .iter()
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>();
    quote! {
        async fn export_csv(State(state): State<AppState>, headers: HeaderMap) -> Result<([(header::HeaderName, String); 1], String), ApiError> {
            let context = state.context(&headers).await?;
            if !state.extensions.#policy().can_list(&context).await? {
                return Err(access_denied(&context));
            }
            let mut select = #module::Entity::find(); #list_scope
            select = select.order_by_asc(#module::Column::#primary_column);
            let total = select.clone().count(&state.database).await?;
            if total > MAX_CSV_EXPORT_ROWS {
                return Err(ApiError::InvalidQuery(format!(
                    "CSV export is limited to {MAX_CSV_EXPORT_ROWS} rows"
                )));
            }
            let columns = [#((#headers, #fields)),*]
                .into_iter()
                .filter(|(_, field)| field_read_allowed(&context, field))
                .collect::<Vec<_>>();
            let mut csv = String::with_capacity(8_192);
            csv.push_str(&columns.iter().map(|(name, _)| csv_escape(name)).collect::<Vec<_>>().join(","));
            csv.push('\n');
            let mut offset = 0;
            while offset < total {
                let models = select.clone().offset(offset).limit(CSV_EXPORT_PAGE_SIZE).all(&state.database).await?;
                for model in models {
                    let value = redact_model(&context, model)?;
                    let object = value.as_object().ok_or(ApiError::Internal)?;
                    let row = columns
                        .iter()
                        .map(|(_, field)| object.get(*field).map(csv_cell).unwrap_or_default())
                        .collect::<Vec<_>>();
                    csv.push_str(&row.iter().map(|value| csv_escape(value)).collect::<Vec<_>>().join(","));
                    csv.push('\n');
                }
                offset += CSV_EXPORT_PAGE_SIZE;
            }
            let _ = context;
            Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned())], csv))
        }
    }
}

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
        ..
    } = context;
    let fields = entity
        .fields
        .iter()
        .filter(|field| field.generated.is_none())
        .collect::<Vec<_>>();
    let field_names = entity
        .fields
        .iter()
        .map(|field| field.api_name.as_str())
        .collect::<Vec<_>>();
    let field_match = fields.iter().map(|field| {
        let name = field.api_name.as_str();
        let kind = kind(&field.ty);
        quote! { #name => { object.insert(#name.to_owned(), csv_json_value(raw, #kind)); } }
    });
    let audit = super::audit_event(*audit_enabled, entity_id, primary, "create");
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
                for (column, raw) in header_row.iter().zip(row.iter()) { match column.as_str() { #(#field_match)* _ => {} } }
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
                    let active = #module::ActiveModel { #(#create_values,)* #active_default };
                    let model = active.insert(&savepoint).await?;
                    state.extensions.#hooks().after_create(&context, &model).await?;
                    #audit
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
        FieldTypeIr::Bigint => "bigint",
        _ => "string",
    }
}
