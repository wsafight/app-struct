use crate::CodegenError;
use appstruct_ir::{EntityIr, FieldTypeIr};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(super) struct BulkContext<'context> {
    pub module: &'context Ident,
    pub primary: &'context Ident,
    pub parse_id: &'context TokenStream,
    pub hooks: &'context Ident,
    pub policy: &'context Ident,
    pub list_scope: &'context TokenStream,
    pub delete_allowed: &'context TokenStream,
    pub update_allowed: &'context TokenStream,
    pub create_allowed: &'context TokenStream,
    pub create_values: &'context [TokenStream],
    pub active_default: Option<&'context TokenStream>,
    pub updates: &'context [TokenStream],
    pub entity_id: &'context str,
    pub audit_enabled: bool,
}

#[allow(clippy::too_many_arguments, clippy::unnecessary_wraps)]
pub(super) fn source(
    entity: &EntityIr,
    module: &Ident,
    parse_id: &TokenStream,
    primary: &Ident,
    hooks: &Ident,
    policy: &Ident,
    list_scope: &TokenStream,
    create_allowed: &TokenStream,
    delete_allowed: &TokenStream,
    update_allowed: &TokenStream,
    create_values: &[TokenStream],
    active_default: Option<&TokenStream>,
    updates: &[TokenStream],
) -> Result<TokenStream, CodegenError> {
    let context = BulkContext {
        module,
        primary,
        parse_id,
        hooks,
        policy,
        list_scope,
        delete_allowed,
        update_allowed,
        create_allowed,
        create_values,
        active_default,
        updates,
        entity_id: &entity.id.0,
        audit_enabled: entity.audit_enabled,
    };
    let update = bulk_update(&context);
    let delete = bulk_delete(&context);
    let export = csv_export(entity, module, list_scope);
    let import = csv_import(entity, &context);
    Ok(quote! {
        #[derive(Clone, Debug, Deserialize)]
        struct BulkUpdateInput { ids: Vec<String>, patch: UpdateInput, expected_revisions: BTreeMap<String, i64> }

        #[derive(Debug, Deserialize)]
        struct BulkDeleteInput { ids: Vec<String>, expected_revisions: BTreeMap<String, i64> }

        #[derive(Debug, Serialize)]
        struct BulkFailure { id: String, code: String, message: String }

        #[derive(Debug, Serialize)]
        struct BulkResult { succeeded: Vec<String>, failed: Vec<BulkFailure> }

        #update
        #delete
        #export
        #import

        fn bulk_failure(id: &str, code: &str, message: impl Into<String>) -> BulkFailure {
            BulkFailure { id: id.to_owned(), code: code.to_owned(), message: message.into() }
        }

        fn csv_escape(value: &str) -> String {
            if value.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", value.replace('"', "\"\""))
            } else {
                value.to_owned()
            }
        }

        fn parse_csv_rows(body: &str) -> Result<Vec<Vec<String>>, ApiError> {
            let mut rows = Vec::new();
            let mut row = Vec::new();
            let mut value = String::new();
            let mut quoted = false;
            let mut chars = body.chars().peekable();
            while let Some(character) = chars.next() {
                match character {
                    '"' if quoted && chars.peek() == Some(&'"') => {
                        value.push('"');
                        chars.next();
                    }
                    '"' => quoted = !quoted,
                    ',' if !quoted => { row.push(std::mem::take(&mut value)); }
                    '\n' if !quoted => {
                        row.push(std::mem::take(&mut value));
                        if !row.iter().all(String::is_empty) { rows.push(std::mem::take(&mut row)); }
                    }
                    '\r' if !quoted => {}
                    _ => value.push(character),
                }
            }
            if quoted { return Err(ApiError::InvalidQuery("CSV contains an unterminated quote".to_owned())); }
            if !value.is_empty() || !row.is_empty() {
                row.push(value);
                if !row.iter().all(String::is_empty) { rows.push(row); }
            }
            Ok(rows)
        }

        fn csv_json_value(value: &str, kind: &str) -> serde_json::Value {
            if value.is_empty() { return serde_json::Value::Null; }
            match kind {
                "boolean" => value.parse::<bool>().map(serde_json::Value::Bool).unwrap_or_else(|_| serde_json::Value::String(value.to_owned())),
                "integer" => value.parse::<i32>().map(|value| serde_json::json!(value)).unwrap_or_else(|_| serde_json::Value::String(value.to_owned())),
                "bigint" => value.parse::<i64>().map(|value| serde_json::json!(value)).unwrap_or_else(|_| serde_json::Value::String(value.to_owned())),
                _ => serde_json::Value::String(value.to_owned()),
            }
        }
    })
}

fn bulk_update(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        primary,
        parse_id,
        hooks,
        policy,
        update_allowed,
        updates,
        list_scope,
        audit_enabled,
        entity_id,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "update");
    quote! {
        async fn bulk_update(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkUpdateInput>,
        ) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            authorize_update_fields(&context, &input.patch)?;
            validate_update(&input.patch)?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = match id_text.parse::<String>() {
                    Ok(_) => { let id = id_text.clone(); #parse_id }
                    Err(_) => { result.failed.push(bulk_failure(id_text, "invalid_id", "invalid record id")); continue; }
                };
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, actor.clone(), tenant);
                let mut select = #module::Entity::find_by_id(id);
                #list_scope
                let model = select.lock_exclusive().one(&transaction).await?;
                let Some(before) = model else {
                    result.failed.push(bulk_failure(id_text, "not_found", "record was not found"));
                    continue;
                };
                if before.revision != expected {
                    result.failed.push(bulk_failure(id_text, "concurrent_modification", "record revision is stale"));
                    continue;
                }
                if !state.extensions.#policy().can_read(&context, &before).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record update is not allowed"));
                    continue;
                }
                let mut input = input.patch.clone();
                state.extensions.#hooks().before_update(&context, &before, &mut input).await?;
                authorize_update_fields(&context, &input)?;
                validate_update(&input)?;
                let mut active = before.clone().into_active_model();
                #(#updates)*
                active.revision = Set(before.revision.checked_add(1).ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                let candidate = active.clone().try_into_model()?;
                if !(#update_allowed) || !state.extensions.#policy().can_update(&context, &before, &input, &candidate).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record update is not allowed by policy"));
                    continue;
                }
                let after = active.update(&transaction).await?;
                state.extensions.#hooks().after_update(&context, &before, &after).await?;
                #audit
                result.succeeded.push(id_text.clone());
            }
            transaction.commit().await?;
            Ok(Json(result))
        }
    }
}

fn bulk_delete(context: &BulkContext<'_>) -> TokenStream {
    let BulkContext {
        module,
        parse_id,
        hooks,
        policy,
        list_scope,
        delete_allowed,
        primary,
        audit_enabled,
        entity_id,
        ..
    } = context;
    let audit = audit_event(*audit_enabled, entity_id, primary, "delete");
    quote! {
        async fn bulk_delete(
            State(state): State<AppState>, headers: HeaderMap,
            Json(input): Json<BulkDeleteInput>,
        ) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for id_text in &input.ids {
                let Some(expected) = input.expected_revisions.get(id_text).copied() else {
                    result.failed.push(bulk_failure(id_text, "precondition_required", "expected_revisions must include every id"));
                    continue;
                };
                let id = { let id = id_text.clone(); #parse_id };
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, actor.clone(), tenant);
                let mut select = #module::Entity::find_by_id(id);
                #list_scope
                let Some(model) = select.lock_exclusive().one(&transaction).await? else {
                    result.failed.push(bulk_failure(id_text, "not_found", "record was not found"));
                    continue;
                };
                if model.revision != expected || !state.extensions.#policy().can_read(&context, &model).await? || !(#delete_allowed) || !state.extensions.#policy().can_delete(&context, &model).await? {
                    result.failed.push(bulk_failure(id_text, "forbidden", "record delete is not allowed"));
                    continue;
                }
                state.extensions.#hooks().before_delete(&context, &model).await?;
                let deleted = model.clone();
                model.delete(&transaction).await?;
                state.extensions.#hooks().after_delete(&context, &deleted).await?;
                #audit
                result.succeeded.push(id_text.clone());
            }
            transaction.commit().await?;
            Ok(Json(result))
        }
    }
}

fn csv_export(entity: &EntityIr, module: &Ident, list_scope: &TokenStream) -> TokenStream {
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
        async fn export_csv(
            State(state): State<AppState>, headers: HeaderMap,
        ) -> Result<([(header::HeaderName, String); 1], String), ApiError> {
            let context = state.context(&headers).await?;
            let mut select = #module::Entity::find();
            #list_scope
            let models = select.all(&state.database).await?;
            let mut csv = String::new();
            csv.push_str(&[#(csv_escape(#headers)),*].join(","));
            csv.push('\n');
            for model in models {
                let value = serde_json::to_value(model).map_err(|_| ApiError::Internal)?;
                let object = value.as_object().ok_or(ApiError::Internal)?;
                let row = [#(object.get(#fields).map(|value| value.to_string()).unwrap_or_default()),*];
                csv.push_str(&row.iter().map(|value| csv_escape(value.trim_matches('"'))).collect::<Vec<_>>().join(","));
                csv.push('\n');
            }
            let _ = context;
            Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned())], csv))
        }
    }
}

fn csv_import(entity: &EntityIr, context: &BulkContext<'_>) -> TokenStream {
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
        let kind = csv_kind(&field.ty);
        quote! { #name => { object.insert(#name.to_owned(), csv_json_value(raw, #kind)); } }
    });
    let audit = audit_event(*audit_enabled, entity_id, primary, "create");
    quote! {
        async fn import_csv(
            State(state): State<AppState>, headers: HeaderMap, body: String,
        ) -> Result<Json<BulkResult>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let context = state.context(&headers).await?;
            let rows = parse_csv_rows(&body)?;
            let Some(header_row) = rows.first() else { return Ok(Json(BulkResult { succeeded: Vec::new(), failed: Vec::new() })); };
            let expected = [#(#field_names),*];
            if header_row.iter().map(String::as_str).any(|name| !expected.contains(&name)) {
                return Err(ApiError::InvalidQuery("CSV contains an unknown column".to_owned()));
            }
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let transaction = state.database.begin().await?;
            let mut result = BulkResult { succeeded: Vec::new(), failed: Vec::new() };
            for (index, row) in rows.iter().skip(1).enumerate() {
                let mut object = serde_json::Map::new();
                for (column, raw) in header_row.iter().zip(row.iter()) {
                    match column.as_str() { #(#field_match)* _ => {} }
                }
                let mut input: CreateInput = match serde_json::from_value(serde_json::Value::Object(object)) {
                    Ok(input) => input,
                    Err(error) => { result.failed.push(bulk_failure(&index.to_string(), "invalid_row", error.to_string())); continue; }
                };
                authorize_create_fields(&context, &input)?;
                validate_create(&input)?;
                let context = RequestContext::transaction_with_file(&transaction, &state.mail, &state.file, actor.clone(), tenant);
                if !(#create_allowed) || !state.extensions.#policy().can_create(&context, &input).await? {
                    result.failed.push(bulk_failure(&index.to_string(), "forbidden", "record creation is not allowed"));
                    continue;
                }
                state.extensions.#hooks().before_create(&context, &mut input).await?;
                let active = #module::ActiveModel { #(#create_values,)* #active_default };
                let model = active.insert(&transaction).await?;
                state.extensions.#hooks().after_create(&context, &model).await?;
                #audit
                result.succeeded.push(index.to_string());
            }
            transaction.commit().await?;
            Ok(Json(result))
        }
    }
}

fn csv_kind(ty: &FieldTypeIr) -> &'static str {
    match ty {
        FieldTypeIr::Boolean => "boolean",
        FieldTypeIr::Integer => "integer",
        FieldTypeIr::Bigint => "bigint",
        _ => "string",
    }
}

fn audit_event(enabled: bool, entity_id: &str, primary: &Ident, operation: &str) -> TokenStream {
    if !enabled {
        return TokenStream::new();
    }
    let entity_id = entity_id.to_owned();
    match operation {
        "create" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, model.#primary.to_string(), "create", None, Some(&model)).await?; }
        }
        "update" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, after.#primary.to_string(), "update", Some(&before), Some(&after)).await?; }
        }
        "delete" => {
            quote! { crate::audit::record(&transaction, &context, #entity_id, deleted.#primary.to_string(), "delete", Some(&deleted), None).await?; }
        }
        _ => TokenStream::new(),
    }
}
