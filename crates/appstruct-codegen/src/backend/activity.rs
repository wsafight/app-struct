use super::{module_name, parse_ident, render};
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

mod attachment;
mod disabled;

use disabled::disabled_source;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.activity.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/activity.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

#[allow(clippy::too_many_lines)]
fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let max_comment_bytes = ir.activity.max_comment_bytes;
    let admin_roles = &ir.activity.admin_roles;
    let admin_allowed = quote! { false #(|| actor.has_role(#admin_roles))* };
    let resource_arms = ir
        .activity
        .resources
        .iter()
        .map(|resource| {
            let entity = ir
                .entities
                .iter()
                .find(|entity| entity.id == resource.entity)
                .expect("IR validation guarantees activity entities exist");
            let module = parse_ident(&module_name(entity))?;
            let key = resource.resource.as_str();
            Ok(quote! {
                #key => crate::api::#module::authorize_activity_target(
                    state, context, record_id,
                ).await
            })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let known_resources = ir
        .activity
        .resources
        .iter()
        .map(|entry| entry.resource.as_str());
    let entry_select = if ir.activity.attachments {
        concat!(
            "SELECT e.id, e.resource, e.record_id, e.tenant_id, e.actor_id, e.kind::text AS kind, e.body, e.event, e.payload, e.attachment_file_id, e.attachment_name, e.attachment_content_type, e.withdrawn_at, e.withdrawn_by, e.governance_reason, e.occurred_at, f.object_key AS attachment_object_key ",
            "FROM \"_appstruct_activity_entries\" e LEFT JOIN \"_appstruct_files\" f ON f.id = e.attachment_file_id "
        )
    } else {
        concat!(
            "SELECT e.id, e.resource, e.record_id, e.tenant_id, e.actor_id, e.kind::text AS kind, e.body, e.event, e.payload, e.attachment_file_id, e.attachment_name, e.attachment_content_type, e.withdrawn_at, e.withdrawn_by, e.governance_reason, e.occurred_at, NULL::text AS attachment_object_key ",
            "FROM \"_appstruct_activity_entries\" e "
        )
    };
    let attachment::Support {
        imports: attachment_imports,
        route: attachment_route,
        contract: attachment_contract,
        input: attachment_input,
        create: attachment_create,
        remember: attachment_remember,
        cleanup: attachment_cleanup,
        download,
    } = attachment::support(ir);

    render(quote! {
        use crate::{ApiError, AppState, RequestContext};
        use axum::{
            Json, Router,
            extract::{Path, Query, State},
            http::HeaderMap,
            routing::get,
        };
        use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
        use serde::{Deserialize, Serialize};
        #attachment_imports

        #[derive(Clone, Debug, Serialize)]
        pub struct ActivityEntry {
            pub id: uuid::Uuid,
            pub resource: String,
            pub record_id: String,
            pub tenant_id: Option<uuid::Uuid>,
            pub actor_id: Option<uuid::Uuid>,
            pub kind: String,
            pub body: Option<String>,
            pub event: Option<String>,
            pub payload: Option<serde_json::Value>,
            pub attachment_file_id: Option<uuid::Uuid>,
            pub attachment_name: Option<String>,
            pub attachment_content_type: Option<String>,
            pub withdrawn_at: Option<chrono::DateTime<chrono::Utc>>,
            pub withdrawn_by: Option<uuid::Uuid>,
            pub governance_reason: Option<String>,
            pub occurred_at: chrono::DateTime<chrono::Utc>,
            #[serde(skip_serializing)]
            attachment_object_key: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct ActivityListQuery { cursor: Option<String>, limit: Option<u64> }
        #[derive(Debug, Serialize)]
        struct ActivityListMeta { limit: u64, next_cursor: Option<String>, has_more: bool }
        #[derive(Debug, Serialize)]
        struct ActivityListResponse { data: Vec<ActivityEntry>, meta: ActivityListMeta }
        #[derive(Debug, Deserialize)]
        struct CreateCommentInput { body: String, #attachment_input }
        #[derive(Debug, Deserialize)]
        struct ModerateInput { reason: String }
        #attachment_contract

        pub fn router() -> Router<AppState> {
            Router::new()
                .route("/api/activity/{resource}/{record_id}", get(list))
                .route(
                    "/api/activity/{resource}/{record_id}/comments",
                    axum::routing::post(create_comment),
                )
                .route(
                    "/api/activity/{resource}/{record_id}/{entry_id}/withdraw",
                    axum::routing::post(withdraw),
                )
                .route(
                    "/api/activity/{resource}/{record_id}/{entry_id}/moderate",
                    axum::routing::post(moderate),
                )
                #attachment_route
        }

        async fn list(
            State(state): State<AppState>, headers: HeaderMap,
            Path((resource, record_id)): Path<(String, String)>,
            Query(query): Query<ActivityListQuery>,
        ) -> Result<Json<ActivityListResponse>, ApiError> {
            let context = state.context(&headers).await?;
            authorize_target(&state, &context, &resource, &record_id).await?;
            let limit = query.limit.unwrap_or(25);
            if !(1..=100).contains(&limit) {
                return Err(ApiError::InvalidQuery("activity limit must be between 1 and 100".to_owned()));
            }
            let fetch_limit = i64::try_from(limit + 1).unwrap_or(101);
            let rows = if let Some(cursor) = query.cursor.as_deref() {
                let (occurred_at, id) = decode_activity_cursor(cursor)?;
                state.database.query_all_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    concat!(
                        #entry_select,
                        "WHERE e.tenant_id IS NOT DISTINCT FROM $1 AND e.resource = $2 AND e.record_id = $3 AND (e.occurred_at, e.id) < ($4, $5) ORDER BY e.occurred_at DESC, e.id DESC LIMIT $6"
                    ),
                    [context.tenant().into(), resource.clone().into(), record_id.clone().into(), occurred_at.into(), id.into(), fetch_limit.into()],
                )).await?
            } else {
                state.database.query_all_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    concat!(
                        #entry_select,
                        "WHERE e.tenant_id IS NOT DISTINCT FROM $1 AND e.resource = $2 AND e.record_id = $3 ORDER BY e.occurred_at DESC, e.id DESC LIMIT $4"
                    ),
                    [context.tenant().into(), resource.into(), record_id.into(), fetch_limit.into()],
                )).await?
            };
            let mut data = rows.into_iter().map(entry_from_row).collect::<Result<Vec<_>, _>>()?;
            let has_more = data.len() > usize::try_from(limit).unwrap_or(100);
            if has_more { data.pop(); }
            let next_cursor = has_more.then(|| data.last().map(encode_activity_cursor)
                .expect("an activity page with more rows is non-empty"));
            Ok(Json(ActivityListResponse { data, meta: ActivityListMeta { limit, next_cursor, has_more } }))
        }

        async fn create_comment(
            State(state): State<AppState>, headers: HeaderMap,
            Path((resource, record_id)): Path<(String, String)>,
            Json(input): Json<CreateCommentInput>,
        ) -> Result<Json<ActivityEntry>, ApiError> {
            let outer = state.mutation_context(&headers).await?;
            let actor = outer.actor().cloned().ok_or(ApiError::Unauthorized)?;
            let tenant_id = outer.tenant();
            let body = sanitize_text(&input.body, #max_comment_bytes, "comment")?;
            let transaction = state.database.begin().await?;
            let context = RequestContext::transaction_with_file(
                &transaction, &state.mail, &state.file, &state.realtime,
                Some(actor.clone()), tenant_id,
            );
            authorize_target(&state, &context, &resource, &record_id).await?;
            let id = uuid::Uuid::now_v7();
            #attachment_create
            let row = transaction.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                concat!(
                    "INSERT INTO \"_appstruct_activity_entries\" (id, resource, record_id, tenant_id, actor_id, kind, body, event, payload, attachment_file_id, attachment_name, attachment_content_type, occurred_at) ",
                    "VALUES ($1, $2, $3, $4, $5, 'comment', $6, NULL, NULL, $7, $8, $9, CURRENT_TIMESTAMP) ",
                    "RETURNING id, resource, record_id, tenant_id, actor_id, kind::text AS kind, body, event, payload, attachment_file_id, attachment_name, attachment_content_type, withdrawn_at, withdrawn_by, governance_reason, occurred_at, NULL::text AS attachment_object_key"
                ),
                [id.into(), resource.into(), record_id.into(), tenant_id.into(), actor.id.into(), body.into(), attachment_file_id.into(), attachment_name.into(), attachment_content_type.into()],
            )).await?.ok_or(ApiError::Internal)?;
            let entry = entry_from_row(row)?;
            transaction.commit().await?;
            publish_activity(&state, &entry, "activity.comment.created");
            Ok(Json(entry))
        }

        async fn withdraw(
            State(state): State<AppState>, headers: HeaderMap,
            Path((resource, record_id, entry_id)): Path<(String, String, String)>,
        ) -> Result<Json<ActivityEntry>, ApiError> {
            let outer = state.mutation_context(&headers).await?;
            let actor = outer.actor().cloned().ok_or(ApiError::Unauthorized)?;
            let tenant_id = outer.tenant();
            let entry_id = uuid::Uuid::parse_str(&entry_id).map_err(|_| ApiError::InvalidId)?;
            let transaction = state.database.begin().await?;
            let context = RequestContext::transaction_with_file(
                &transaction, &state.mail, &state.file, &state.realtime,
                Some(actor.clone()), tenant_id,
            );
            authorize_target(&state, &context, &resource, &record_id).await?;
            let before = load_entry(&transaction, entry_id, tenant_id, &resource, &record_id, true).await?;
            if before.kind != "comment" || before.withdrawn_at.is_some() {
                return Err(ApiError::ActivityAlreadyWithdrawn);
            }
            if before.actor_id != Some(actor.id) { return Err(ApiError::Forbidden); }
            #attachment_remember
            let after = withdraw_entry(&transaction, &before, actor.id, None).await?;
            crate::audit::record(
                &transaction, &context, "appstruct::activity::entry", after.id.to_string(),
                "activity.withdraw", Some(&before), Some(&after),
            ).await?;
            transaction.commit().await?;
            #attachment_cleanup
            publish_activity(&state, &after, "activity.comment.withdrawn");
            Ok(Json(after))
        }

        async fn moderate(
            State(state): State<AppState>, headers: HeaderMap,
            Path((resource, record_id, entry_id)): Path<(String, String, String)>,
            Json(input): Json<ModerateInput>,
        ) -> Result<Json<ActivityEntry>, ApiError> {
            let outer = state.mutation_context(&headers).await?;
            let actor = outer.actor().cloned().ok_or(ApiError::Unauthorized)?;
            if !(#admin_allowed) { return Err(ApiError::Forbidden); }
            let reason = sanitize_text(&input.reason, 1_000, "moderation reason")?;
            let tenant_id = outer.tenant();
            let entry_id = uuid::Uuid::parse_str(&entry_id).map_err(|_| ApiError::InvalidId)?;
            let transaction = state.database.begin().await?;
            let context = RequestContext::transaction_with_file(
                &transaction, &state.mail, &state.file, &state.realtime,
                Some(actor.clone()), tenant_id,
            );
            authorize_target(&state, &context, &resource, &record_id).await?;
            let before = load_entry(&transaction, entry_id, tenant_id, &resource, &record_id, true).await?;
            if before.kind != "comment" || before.withdrawn_at.is_some() {
                return Err(ApiError::ActivityAlreadyWithdrawn);
            }
            #attachment_remember
            let after = withdraw_entry(&transaction, &before, actor.id, Some(reason)).await?;
            crate::audit::record(
                &transaction, &context, "appstruct::activity::entry", after.id.to_string(),
                "activity.moderate", Some(&before), Some(&after),
            ).await?;
            transaction.commit().await?;
            #attachment_cleanup
            publish_activity(&state, &after, "activity.comment.moderated");
            Ok(Json(after))
        }

        async fn authorize_target(
            state: &AppState, context: &RequestContext<'_>, resource: &str, record_id: &str,
        ) -> Result<(), ApiError> {
            if record_id.is_empty() || record_id.len() > 255 {
                return Err(ApiError::InvalidId);
            }
            match resource {
                #(#resource_arms,)*
                _ => Err(ApiError::UnknownActivityResource),
            }
        }

        pub(crate) async fn record_system_event<C: ConnectionTrait>(
            database: &C, context: &RequestContext<'_>, resource: &str,
            record_id: String, event: &str,
        ) -> Result<(), ApiError> {
            match resource {
                #(#known_resources)|* => {}
                _ => return Err(ApiError::UnknownActivityResource),
            }
            database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_activity_entries\" (id, resource, record_id, tenant_id, actor_id, kind, body, event, payload, occurred_at) VALUES ($1, $2, $3, $4, $5, 'system', NULL, $6, NULL, CURRENT_TIMESTAMP)",
                [uuid::Uuid::now_v7().into(), resource.to_owned().into(), record_id.into(), context.tenant().into(), context.actor().map(|actor| actor.id).into(), event.to_owned().into()],
            )).await?;
            Ok(())
        }

        async fn withdraw_entry<C: ConnectionTrait>(
            database: &C, before: &ActivityEntry, actor_id: uuid::Uuid, reason: Option<String>,
        ) -> Result<ActivityEntry, ApiError> {
            database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_activity_entries\" SET body = NULL, attachment_file_id = NULL, attachment_name = NULL, attachment_content_type = NULL, withdrawn_at = CURRENT_TIMESTAMP, withdrawn_by = $2, governance_reason = $3 WHERE id = $1",
                [before.id.into(), actor_id.into(), reason.into()],
            )).await?;
            load_entry(
                database, before.id, before.tenant_id, &before.resource, &before.record_id, false,
            ).await
        }

        async fn load_entry<C: ConnectionTrait>(
            database: &C, id: uuid::Uuid, tenant_id: Option<uuid::Uuid>,
            resource: &str, record_id: &str, locked: bool,
        ) -> Result<ActivityEntry, ApiError> {
            let suffix = if locked { " FOR UPDATE OF e" } else { "" };
            let sql = format!(
                "{}{}",
                concat!(
                    #entry_select,
                    "WHERE e.id = $1 AND e.tenant_id IS NOT DISTINCT FROM $2 AND e.resource = $3 AND e.record_id = $4"
                ),
                suffix,
            );
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres, sql,
                [id.into(), tenant_id.into(), resource.to_owned().into(), record_id.to_owned().into()],
            )).await?.ok_or(ApiError::NotFound)?;
            Ok(entry_from_row(row)?)
        }

        fn entry_from_row(row: sea_orm::QueryResult) -> Result<ActivityEntry, sea_orm::DbErr> {
            Ok(ActivityEntry {
                id: row.try_get("", "id")?, resource: row.try_get("", "resource")?,
                record_id: row.try_get("", "record_id")?, tenant_id: row.try_get("", "tenant_id")?,
                actor_id: row.try_get("", "actor_id")?, kind: row.try_get("", "kind")?,
                body: row.try_get("", "body")?, event: row.try_get("", "event")?,
                payload: row.try_get("", "payload")?, attachment_file_id: row.try_get("", "attachment_file_id")?,
                attachment_name: row.try_get("", "attachment_name")?,
                attachment_content_type: row.try_get("", "attachment_content_type")?,
                withdrawn_at: row.try_get("", "withdrawn_at")?, withdrawn_by: row.try_get("", "withdrawn_by")?,
                governance_reason: row.try_get("", "governance_reason")?, occurred_at: row.try_get("", "occurred_at")?,
                attachment_object_key: row.try_get("", "attachment_object_key")?,
            })
        }

        fn sanitize_text(value: &str, maximum: u32, field: &str) -> Result<String, ApiError> {
            let value = value.chars()
                .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
                .collect::<String>();
            let value = value.trim().to_owned();
            if value.is_empty() || value.len() > usize::try_from(maximum).unwrap_or(usize::MAX) {
                return Err(ApiError::InvalidActivityInput(format!(
                    "{field} must contain between 1 and {maximum} bytes",
                )));
            }
            Ok(value)
        }

        fn encode_activity_cursor(entry: &ActivityEntry) -> String {
            appstruct_runtime::encode_cursor(&format!(
                "{}|{}",
                entry.occurred_at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true), entry.id,
            ))
        }

        fn decode_activity_cursor(
            cursor: &str,
        ) -> Result<(chrono::DateTime<chrono::Utc>, uuid::Uuid), ApiError> {
            let raw = appstruct_runtime::decode_cursor(cursor)
                .ok_or_else(|| ApiError::InvalidQuery("invalid activity cursor".to_owned()))?;
            let (occurred_at, id) = raw.split_once('|')
                .ok_or_else(|| ApiError::InvalidQuery("invalid activity cursor".to_owned()))?;
            let occurred_at = chrono::DateTime::parse_from_rfc3339(occurred_at)
                .map_err(|_| ApiError::InvalidQuery("invalid activity cursor".to_owned()))?
                .with_timezone(&chrono::Utc);
            let id = uuid::Uuid::parse_str(id)
                .map_err(|_| ApiError::InvalidQuery("invalid activity cursor".to_owned()))?;
            Ok((occurred_at, id))
        }

        fn publish_activity(state: &AppState, entry: &ActivityEntry, event: &str) {
            if let Err(error) = state.realtime.publish_resource_model(
                event, &entry.resource, &entry.record_id, entry, entry.actor_id, entry.tenant_id,
            ) {
                tracing::warn!(%error, "activity realtime event was not published");
            }
        }

        #download
    })
}
