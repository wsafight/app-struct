use super::BulkContext;
use appstruct_ir::{EntityIr, FieldTypeIr};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(super) fn export(
    entity: &EntityIr,
    module: &Ident,
    policy: &Ident,
    list_scope: &TokenStream,
) -> TokenStream {
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
            let models = select.all(&state.database).await?;
            let mut csv = String::new(); csv.push_str(&[#(csv_escape(#headers)),*].join(",")); csv.push('\n');
            for model in models {
                let value = serde_json::to_value(model).map_err(|_| ApiError::Internal)?;
                let object = value.as_object().ok_or(ApiError::Internal)?;
                let row = [#(object.get(#fields).map(std::string::ToString::to_string).unwrap_or_default()),*];
                csv.push_str(&row.iter().map(|value| csv_escape(value.trim_matches('"'))).collect::<Vec<_>>().join(",")); csv.push('\n');
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
    quote! {
        async fn import_csv(State(state): State<AppState>, headers: HeaderMap, body: String) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            let rows = parse_csv_rows(&body)
                .map_err(|error| ApiError::InvalidQuery(error.to_string()))?;
            let Some(header_row) = rows.first() else { return Ok(Json(BulkResult { succeeded: Vec::new(), failed: Vec::new() })); };
            let expected = [#(#field_names),*];
            if header_row.iter().map(String::as_str).any(|name| !expected.contains(&name)) { return Err(ApiError::InvalidQuery("CSV contains an unknown column".to_owned())); }
            let actor = context.actor().cloned(); let tenant = context.tenant(); let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for (index, row) in rows.iter().skip(1).enumerate() {
                let mut object = serde_json::Map::new();
                for (column, raw) in header_row.iter().zip(row.iter()) { match column.as_str() { #(#field_match)* _ => {} } }
                let mut input: CreateInput = match serde_json::from_value(serde_json::Value::Object(object)) { Ok(input) => input, Err(error) => { result.failed.push(bulk_failure(&index.to_string(), "invalid_row", error.to_string())); continue; } };
                authorize_create_fields(&context, &input)?; validate_create(&input)?;
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant);
                if !(#create_allowed) || !state.extensions.#policy().can_create(&context, &input).await? { result.failed.push(bulk_failure(&index.to_string(), "forbidden", "record creation is not allowed")); continue; }
                state.extensions.#hooks().before_create(&context, &mut input).await?;
                let active = #module::ActiveModel { #(#create_values,)* #active_default }; let model = active.insert(&transaction).await?;
                state.extensions.#hooks().after_create(&context, &model).await?; #audit result.succeeded.push(index.to_string());
            }
            transaction.commit().await?; Ok(Json(result))
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
