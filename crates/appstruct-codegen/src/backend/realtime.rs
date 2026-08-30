use super::{module_name, parse_ident, render};
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

mod fanout;
mod locks;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.realtime.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/realtime.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

#[allow(clippy::too_many_lines)]
fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let heartbeat_seconds = ir.realtime.heartbeat_seconds;
    let ttl_seconds = i64::try_from(ir.realtime.presence_ttl_seconds).unwrap_or(45);
    let mut scope_arms = Vec::new();
    let mut event_arms = Vec::new();
    let mut inferred_resources = Vec::new();
    for entity in &ir.entities {
        let module_name = module_name(entity);
        let module = parse_ident(&module_name)?;
        let resource = &entity.table_name;
        scope_arms.push(quote! {
            #resource => crate::api::#module::authorize_realtime_scope(
                state, context, record_id,
            ).await
        });
        event_arms.push(quote! {
            Some(#resource) => crate::api::#module::authorize_realtime_event(
                state, context, event,
            ).await
        });
        inferred_resources.push(quote! { #module_name => Some(#resource) });
    }
    let fanout = fanout::state(&inferred_resources);
    let locks = locks::support();
    render(quote! {
        use crate::{ApiError, AppState};
        use axum::{
            Router,
            extract::{Query, State},
            http::HeaderMap,
            response::sse::{Event, KeepAlive, Sse},
            routing::get,
        };
        use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
        use serde::{Deserialize, Serialize};
        use std::{convert::Infallible, time::Duration};
        use tokio::sync::broadcast;

        #fanout
        #locks

        #[derive(Debug, Deserialize)]
        struct RealtimeQuery {
            tenant_id: Option<uuid::Uuid>,
            resource: Option<String>,
            record_id: Option<String>,
        }
        #[derive(Debug, Serialize)]
        pub struct PresenceEntry {
            pub connection_id: uuid::Uuid, pub actor_id: uuid::Uuid,
            pub tenant_id: Option<uuid::Uuid>, pub resource: Option<String>,
            pub record_id: Option<String>, pub connected_at: chrono::DateTime<chrono::Utc>,
            pub last_seen_at: chrono::DateTime<chrono::Utc>,
            pub expires_at: chrono::DateTime<chrono::Utc>,
        }
        #[derive(Serialize)] struct PresenceList { data: Vec<PresenceEntry> }

        pub fn router() -> Router<AppState> {
            Router::new()
                .route("/api/realtime/events", get(events))
                .route("/api/realtime/presence", get(presence))
                .route("/api/realtime/locks", get(lock_status).post(acquire_lock))
                .route("/api/realtime/locks/{token}", axum::routing::patch(renew_lock).delete(release_lock))
        }

        async fn events(
            State(state): State<AppState>, headers: HeaderMap, Query(query): Query<RealtimeQuery>,
        ) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
            validate_scope(&query)?;
            let context = scoped_context(&state, headers, query.tenant_id).await?;
            let actor = context.actor().ok_or(ApiError::Unauthorized)?.clone();
            let tenant_id = context.tenant();
            let resource = query.resource.clone().ok_or_else(|| {
                ApiError::InvalidQuery("realtime resource is required".to_owned())
            })?;
            authorize_resource_scope(
                &state, &context, &resource, query.record_id.as_deref(),
            ).await?;
            let connection_id = uuid::Uuid::now_v7();
            register_presence(
                &state.database, connection_id, actor.id, tenant_id,
                &resource, query.record_id.as_deref(),
            ).await?;
            let mut receiver = state.realtime.subscribe();
            let realtime = state.realtime.clone();
            let database = state.database.clone();
            let record_id = query.record_id;
            let event_state = state.clone();
            let event_actor = actor.clone();
            let stream = async_stream::stream! {
                let _lease = PresenceLease {
                    database: database.clone(), realtime: realtime.clone(), connection_id,
                    actor_id: actor.id, tenant_id, resource: resource.clone(), record_id: record_id.clone(),
                };
                let joined = realtime.publish_scoped(
                    "presence.online", Some(&resource), record_id.as_deref(),
                    &presence_payload(connection_id, actor.id, &resource, record_id.as_deref()),
                    Some(actor.id), tenant_id,
                ).ok();
                if let Some(joined) = joined { yield Ok(sse_event(&joined)); }
                let mut heartbeat = tokio::time::interval(Duration::from_secs(#heartbeat_seconds));
                heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = heartbeat.tick() => {
                            if heartbeat_presence(&database, connection_id).await.is_err() { break; }
                        }
                        received = receiver.recv() => match received {
                            Ok(event) if event_matches_scope(&event, tenant_id, &resource, record_id.as_deref()) => {
                                let allowed = if event.resource_model {
                                    let context = crate::RequestContext::connection_with_services(
                                        &event_state.database, &event_state.mail, &event_state.file,
                                        &event_state.realtime, Some(event_actor.clone()), tenant_id,
                                    );
                                    authorize_resource_event(&event_state, &context, &event)
                                        .await.unwrap_or(false)
                                } else {
                                    true
                                };
                                if allowed { yield Ok(sse_event(&event)); }
                            }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                yield Ok(Event::default().event("resync").data("{}"));
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            };
            Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(#heartbeat_seconds))))
        }

        async fn presence(
            State(state): State<AppState>, headers: HeaderMap, Query(query): Query<RealtimeQuery>,
        ) -> Result<axum::Json<PresenceList>, ApiError> {
            validate_scope(&query)?;
            let context = scoped_context(&state, headers, query.tenant_id).await?;
            if context.actor().is_none() { return Err(ApiError::Unauthorized); }
            let resource = query.resource.as_deref().ok_or_else(|| {
                ApiError::InvalidQuery("realtime resource is required".to_owned())
            })?;
            authorize_resource_scope(&state, &context, resource, query.record_id.as_deref()).await?;
            let rows = state.database.query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT connection_id, actor_id, tenant_id, resource, record_id, connected_at, last_seen_at, expires_at FROM \"_appstruct_realtime_presence\" WHERE expires_at > CURRENT_TIMESTAMP AND (($1::uuid IS NULL AND tenant_id IS NULL) OR tenant_id = $1) AND resource = $2 AND (($3::text IS NULL AND record_id IS NULL) OR record_id = $3) ORDER BY connected_at, connection_id LIMIT 500",
                [context.tenant().into(), query.resource.into(), query.record_id.into()],
            )).await?;
            let data = rows.into_iter().map(presence_from_row)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(axum::Json(PresenceList { data }))
        }

        async fn scoped_context<'state>(
            state: &'state AppState, mut headers: HeaderMap, tenant_id: Option<uuid::Uuid>,
        ) -> Result<crate::RequestContext<'state>, ApiError> {
            if let Some(tenant_id) = tenant_id {
                let value = tenant_id.to_string().parse().map_err(|_| ApiError::InvalidTenant)?;
                headers.insert("x-appstruct-tenant", value);
            }
            state.context(&headers).await
        }

        fn validate_scope(query: &RealtimeQuery) -> Result<(), ApiError> {
            if query.resource.as_ref().is_none_or(|value| value.is_empty() || value.len() > 120)
                || query.record_id.as_ref().is_some_and(|value| value.is_empty() || value.len() > 200)
            {
                return Err(ApiError::InvalidQuery("invalid presence scope".to_owned()));
            }
            Ok(())
        }

        async fn authorize_resource_scope(
            state: &AppState, context: &crate::RequestContext<'_>,
            resource: &str, record_id: Option<&str>,
        ) -> Result<(), ApiError> {
            match resource {
                #(#scope_arms,)*
                _ => Err(ApiError::InvalidQuery("unknown realtime resource".to_owned())),
            }
        }

        async fn authorize_resource_event(
            state: &AppState, context: &crate::RequestContext<'_>, event: &RealtimeEvent,
        ) -> Result<bool, ApiError> {
            match event.resource.as_deref() {
                #(#event_arms,)*
                _ => Ok(false),
            }
        }

        fn event_matches_scope(
            event: &RealtimeEvent, tenant_id: Option<uuid::Uuid>, resource: &str,
            record_id: Option<&str>,
        ) -> bool {
            if event.tenant_id != tenant_id || event.resource.as_deref() != Some(resource) {
                return false;
            }
            if let Some(record_id) = record_id {
                return event.record_id.as_deref() == Some(record_id);
            }
            !event.event.starts_with("presence.") || event.record_id.is_none()
        }

        async fn register_presence(
            database: &DatabaseConnection, connection_id: uuid::Uuid, actor_id: uuid::Uuid,
            tenant_id: Option<uuid::Uuid>, resource: &str, record_id: Option<&str>,
        ) -> Result<(), DbErr> {
            database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_realtime_presence\" (connection_id, actor_id, tenant_id, resource, record_id, connected_at, last_seen_at, expires_at) VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + ($6 * INTERVAL '1 second'))",
                [connection_id.into(), actor_id.into(), tenant_id.into(), resource.to_owned().into(), record_id.map(str::to_owned).into(), #ttl_seconds.into()],
            )).await?;
            Ok(())
        }
        async fn heartbeat_presence(database: &DatabaseConnection, connection_id: uuid::Uuid) -> Result<(), DbErr> {
            let result = database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_realtime_presence\" SET last_seen_at = CURRENT_TIMESTAMP, expires_at = CURRENT_TIMESTAMP + ($2 * INTERVAL '1 second') WHERE connection_id = $1",
                [connection_id.into(), #ttl_seconds.into()],
            )).await?;
            database.execute_unprepared("DELETE FROM \"_appstruct_realtime_presence\" WHERE expires_at <= CURRENT_TIMESTAMP").await?;
            if result.rows_affected() == 1 { Ok(()) } else { Err(DbErr::Custom("presence lease was lost".to_owned())) }
        }

        struct PresenceLease {
            database: DatabaseConnection, realtime: RealtimeState, connection_id: uuid::Uuid,
            actor_id: uuid::Uuid, tenant_id: Option<uuid::Uuid>, resource: String,
            record_id: Option<String>,
        }
        impl Drop for PresenceLease {
            fn drop(&mut self) {
                let database = self.database.clone();
                let realtime = self.realtime.clone();
                let connection_id = self.connection_id;
                let actor_id = self.actor_id;
                let tenant_id = self.tenant_id;
                let resource = self.resource.clone();
                let record_id = self.record_id.clone();
                tokio::spawn(async move {
                    let _ = database.execute_raw(Statement::from_sql_and_values(
                        DbBackend::Postgres,
                        "DELETE FROM \"_appstruct_realtime_presence\" WHERE connection_id = $1",
                        [connection_id.into()],
                    )).await;
                    let _ = realtime.publish_scoped(
                        "presence.offline", Some(&resource), record_id.as_deref(),
                        &presence_payload(connection_id, actor_id, &resource, record_id.as_deref()),
                        Some(actor_id), tenant_id,
                    );
                });
            }
        }

        fn presence_payload(
            connection_id: uuid::Uuid, actor_id: uuid::Uuid,
            resource: &str, record_id: Option<&str>,
        ) -> serde_json::Value {
            serde_json::json!({ "connection_id": connection_id, "actor_id": actor_id, "resource": resource, "record_id": record_id })
        }
        fn sse_event(event: &RealtimeEvent) -> Event {
            let mut event = event.clone();
            if event.resource_model {
                event.data = serde_json::json!({
                    "resource": event.resource.clone(),
                    "record_id": event.record_id.clone(),
                });
            }
            Event::default().id(event.id.to_string()).event(&event.event)
                .json_data(&event).unwrap_or_else(|_| Event::default().event("serialization_error"))
        }
        fn presence_from_row(row: sea_orm::QueryResult) -> Result<PresenceEntry, DbErr> {
            Ok(PresenceEntry {
                connection_id: row.try_get("", "connection_id")?, actor_id: row.try_get("", "actor_id")?,
                tenant_id: row.try_get("", "tenant_id")?, resource: row.try_get("", "resource")?,
                record_id: row.try_get("", "record_id")?, connected_at: row.try_get("", "connected_at")?,
                last_seen_at: row.try_get("", "last_seen_at")?, expires_at: row.try_get("", "expires_at")?,
            })
        }
    })
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use crate::AppState;
        use axum::Router;
        use sea_orm::DatabaseConnection;
        use serde::Serialize;
        #[derive(Clone, Debug, Serialize)]
        pub struct RealtimeEvent;
        #[derive(Clone, Default)]
        pub struct RealtimeState;
        impl RealtimeState {
            pub(crate) fn new(_database: DatabaseConnection) -> Self { Self }
            pub fn publish<T: Serialize>(
                &self, _event: &str, _data: &T, _actor_id: Option<uuid::Uuid>,
                _tenant_id: Option<uuid::Uuid>,
            ) -> Result<RealtimeEvent, serde_json::Error> { Ok(RealtimeEvent) }
            pub(crate) fn publish_resource_model<T: Serialize>(
                &self, _event: &str, _resource: &str, _record_id: &str, _data: &T,
                _actor_id: Option<uuid::Uuid>, _tenant_id: Option<uuid::Uuid>,
            ) -> Result<RealtimeEvent, serde_json::Error> { Ok(RealtimeEvent) }
        }
        pub fn router() -> Router<AppState> { Router::new() }
    })
}
