use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) struct Support {
    pub imports: TokenStream,
    pub route: TokenStream,
    pub contract: TokenStream,
    pub input: TokenStream,
    pub create: TokenStream,
    pub remember: TokenStream,
    pub cleanup: TokenStream,
    pub download: TokenStream,
}

#[allow(clippy::too_many_lines)]
pub(super) fn support(ir: &AppIr) -> Support {
    if !ir.activity.attachments {
        return Support {
            imports: TokenStream::new(),
            route: TokenStream::new(),
            contract: TokenStream::new(),
            input: TokenStream::new(),
            create: quote! {
                let attachment_file_id: Option<uuid::Uuid> = None;
                let attachment_name: Option<String> = None;
                let attachment_content_type: Option<String> = None;
            },
            remember: TokenStream::new(),
            cleanup: TokenStream::new(),
            download: TokenStream::new(),
        };
    }
    let max_encoded = ir
        .file
        .max_bytes
        .saturating_mul(4)
        .saturating_div(3)
        .saturating_add(8);
    Support {
        imports: quote! {
            use axum::{
                body::Body,
                http::{HeaderValue, header},
                response::Response,
            };
            use base64::Engine as _;
        },
        route: quote! {
            .route(
                "/api/activity/{resource}/{record_id}/{entry_id}/attachment",
                get(download_attachment),
            )
        },
        contract: quote! {
            #[derive(Debug, Deserialize)]
            struct ActivityAttachmentInput {
                name: String,
                content_type: String,
                content_base64: String,
            }
        },
        input: quote! { attachment: Option<ActivityAttachmentInput>, },
        create: quote! {
            let (attachment_file_id, attachment_name, attachment_content_type) =
                if let Some(attachment) = input.attachment {
                    if u64::try_from(attachment.content_base64.len()).unwrap_or(u64::MAX) > #max_encoded {
                        return Err(ApiError::InvalidActivityInput(
                            "attachment exceeds the configured file size limit".to_owned(),
                        ));
                    }
                    let content = base64::engine::general_purpose::STANDARD
                        .decode(attachment.content_base64)
                        .map_err(|_| ApiError::InvalidActivityInput(
                            "attachment content_base64 is invalid".to_owned(),
                        ))?;
                    let tenant_scope = tenant_id.map_or_else(|| "global".to_owned(), |id| id.to_string());
                    let object_key = format!("activity/{tenant_scope}/{id}");
                    let metadata = state.file.put_with_connection(
                        &transaction, &object_key, &attachment.name, &attachment.content_type,
                        &content, tenant_id,
                    ).await.map_err(|error| {
                        tracing::warn!(%error, "activity attachment was rejected");
                        ApiError::InvalidActivityInput("attachment is invalid".to_owned())
                    })?;
                    (Some(metadata.id), Some(attachment.name), Some(attachment.content_type))
                } else {
                    (None, None, None)
                };
        },
        remember: quote! {
            let withdrawn_attachment_object_key = before.attachment_object_key.clone();
        },
        cleanup: quote! {
            if let Some(object_key) = withdrawn_attachment_object_key.as_deref() {
                if let Err(error) = state.file.delete(object_key, tenant_id).await {
                    tracing::error!(%error, entry_id = %after.id, "activity attachment cleanup failed");
                }
            }
        },
        download: quote! {
            async fn download_attachment(
                State(state): State<AppState>, headers: HeaderMap,
                Path((resource, record_id, entry_id)): Path<(String, String, String)>,
            ) -> Result<Response, ApiError> {
                let context = state.context(&headers).await?;
                authorize_target(&state, &context, &resource, &record_id).await?;
                let entry_id = uuid::Uuid::parse_str(&entry_id).map_err(|_| ApiError::InvalidId)?;
                let entry = load_entry(
                    &state.database, entry_id, context.tenant(), &resource, &record_id, false,
                ).await?;
                if entry.withdrawn_at.is_some() { return Err(ApiError::NotFound); }
                let object_key = entry.attachment_object_key.as_deref().ok_or(ApiError::NotFound)?;
                let (_, content) = state.file.get(object_key, context.tenant()).await
                    .map_err(|_| ApiError::NotFound)?;
                let content_type = entry.attachment_content_type.as_deref()
                    .and_then(|value| HeaderValue::from_str(value).ok())
                    .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
                let filename = entry.attachment_name.as_deref().unwrap_or("attachment");
                let disposition = HeaderValue::from_str(&format!(
                    "attachment; filename=\"{}\"", filename.replace(['\r', '\n', '\"'], "_")
                )).map_err(|_| ApiError::Internal)?;
                let mut response = Response::new(Body::from(content));
                response.headers_mut().insert(header::CONTENT_TYPE, content_type);
                response.headers_mut().insert(header::CONTENT_DISPOSITION, disposition);
                Ok(response)
            }
        },
    }
}
