use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn state(inferred_resources: &[TokenStream]) -> TokenStream {
    let definitions = definitions();
    let state = state_impl();
    let persistence = persistence(inferred_resources);
    let fanout = fanout();
    quote! {
        #definitions
        #state
        #persistence
        #fanout
    }
}

fn definitions() -> TokenStream {
    quote! {
        #[derive(Clone, Debug, Serialize)]
        pub struct RealtimeEvent {
            pub id: uuid::Uuid,
            pub event: String,
            pub data: serde_json::Value,
            pub resource: Option<String>,
            pub record_id: Option<String>,
            pub actor_id: Option<uuid::Uuid>,
            pub tenant_id: Option<uuid::Uuid>,
            pub occurred_at: chrono::DateTime<chrono::Utc>,
            #[serde(skip_serializing)]
            resource_model: bool,
        }

        struct RealtimeInner {
            sender: broadcast::Sender<RealtimeEvent>,
            database: Option<DatabaseConnection>,
            source_id: uuid::Uuid,
        }

        #[derive(Clone)]
        pub struct RealtimeState { inner: std::sync::Arc<RealtimeInner> }
    }
}

fn state_impl() -> TokenStream {
    quote! {
        impl Default for RealtimeState {
            fn default() -> Self { Self::build(None) }
        }
        impl RealtimeState {
            pub(crate) fn new(database: DatabaseConnection) -> Self {
                let state = Self::build(Some(database));
                state.spawn_fanout();
                state
            }
            fn build(database: Option<DatabaseConnection>) -> Self {
                let (sender, _) = broadcast::channel(1_024);
                Self { inner: std::sync::Arc::new(RealtimeInner {
                    sender, database, source_id: uuid::Uuid::now_v7(),
                }) }
            }
            fn spawn_fanout(&self) {
                let weak = std::sync::Arc::downgrade(&self.inner);
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(fanout_events(weak));
                } else {
                    tracing::warn!("realtime database fan-out requires a Tokio runtime");
                }
            }
            pub fn publish<T: Serialize>(
                &self, event: &str, data: &T, actor_id: Option<uuid::Uuid>,
                tenant_id: Option<uuid::Uuid>,
            ) -> Result<RealtimeEvent, serde_json::Error> {
                let resource = infer_resource(event);
                self.publish_scoped(event, resource, None, data, actor_id, tenant_id)
            }
            fn publish_scoped<T: Serialize>(
                &self, event: &str, resource: Option<&str>, record_id: Option<&str>,
                data: &T, actor_id: Option<uuid::Uuid>, tenant_id: Option<uuid::Uuid>,
            ) -> Result<RealtimeEvent, serde_json::Error> {
                self.dispatch(RealtimeEvent {
                    id: uuid::Uuid::now_v7(), event: event.to_owned(),
                    data: serde_json::to_value(data)?, resource: resource.map(str::to_owned),
                    record_id: record_id.map(str::to_owned), actor_id, tenant_id,
                    occurred_at: chrono::Utc::now(), resource_model: false,
                })
            }
            pub(crate) fn publish_resource_model<T: Serialize>(
                &self, event: &str, resource: &str, record_id: &str, data: &T,
                actor_id: Option<uuid::Uuid>, tenant_id: Option<uuid::Uuid>,
            ) -> Result<RealtimeEvent, serde_json::Error> {
                self.dispatch(RealtimeEvent {
                    id: uuid::Uuid::now_v7(), event: event.to_owned(),
                    data: serde_json::to_value(data)?, resource: Some(resource.to_owned()),
                    record_id: Some(record_id.to_owned()), actor_id, tenant_id,
                    occurred_at: chrono::Utc::now(), resource_model: true,
                })
            }
            fn dispatch(
                &self, event: RealtimeEvent,
            ) -> Result<RealtimeEvent, serde_json::Error> {
                let _ = self.inner.sender.send(event.clone());
                if let Some(database) = self.inner.database.clone() {
                    let persisted = event.clone();
                    let source_id = self.inner.source_id;
                    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                        runtime.spawn(async move {
                            if let Err(error) = persist_event(&database, source_id, &persisted).await {
                                tracing::warn!(?error, event = %persisted.event, "realtime event persistence failed");
                            }
                        });
                    }
                }
                Ok(event)
            }
            fn subscribe(&self) -> broadcast::Receiver<RealtimeEvent> {
                self.inner.sender.subscribe()
            }
        }
    }
}

fn persistence(inferred_resources: &[TokenStream]) -> TokenStream {
    quote! {
        fn infer_resource(event: &str) -> Option<&'static str> {
            match event.split_once('.').map_or(event, |(prefix, _)| prefix) {
                #(#inferred_resources,)*
                _ => None,
            }
        }

        async fn persist_event(
            database: &DatabaseConnection, source_id: uuid::Uuid, event: &RealtimeEvent,
        ) -> Result<(), DbErr> {
            database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_realtime_events\" (id, source_id, event, data, resource, record_id, actor_id, tenant_id, occurred_at, resource_model) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                [event.id.into(), source_id.into(), event.event.clone().into(),
                 event.data.clone().into(), event.resource.clone().into(),
                 event.record_id.clone().into(), event.actor_id.into(), event.tenant_id.into(),
                 event.occurred_at.into(), event.resource_model.into()],
            )).await?;
            Ok(())
        }
    }
}

fn fanout() -> TokenStream {
    quote! {
        async fn fanout_events(inner: std::sync::Weak<RealtimeInner>) {
            let mut sequence = None;
            let mut idle_iterations = 0_u16;
            loop {
                let Some(inner) = inner.upgrade() else { break };
                let Some(database) = inner.database.clone() else { break };
                let source_id = inner.source_id;
                let sender = inner.sender.clone();
                drop(inner);
                if sequence.is_none() {
                    match latest_event_sequence(&database).await {
                        Ok(latest) => sequence = Some(latest),
                        Err(error) => {
                            tracing::warn!(?error, "realtime fan-out cursor initialization failed");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            continue;
                        }
                    }
                }
                match load_events(&database, sequence.unwrap_or_default()).await {
                    Ok(events) => {
                        let count = events.len();
                        for (next_sequence, event_source, event) in events {
                            sequence = Some(next_sequence);
                            if event_source != source_id { let _ = sender.send(event); }
                        }
                        if count == 256 { continue; }
                    }
                    Err(error) => tracing::warn!(?error, "realtime fan-out poll failed"),
                }
                idle_iterations = idle_iterations.wrapping_add(1);
                if idle_iterations % 600 == 0 {
                    let _ = database.execute_unprepared(
                        "DELETE FROM \"_appstruct_realtime_events\" WHERE occurred_at < CURRENT_TIMESTAMP - INTERVAL '5 minutes'",
                    ).await;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        async fn latest_event_sequence(database: &DatabaseConnection) -> Result<i64, DbErr> {
            let row = database.query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT COALESCE(MAX(sequence), 0) AS sequence FROM \"_appstruct_realtime_events\"".to_owned(),
            )).await?.ok_or_else(|| DbErr::Custom("realtime cursor query returned no row".to_owned()))?;
            row.try_get("", "sequence")
        }

        async fn load_events(
            database: &DatabaseConnection, sequence: i64,
        ) -> Result<Vec<(i64, uuid::Uuid, RealtimeEvent)>, DbErr> {
            let rows = database.query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT sequence, id, source_id, event, data, resource, record_id, actor_id, tenant_id, occurred_at, resource_model FROM \"_appstruct_realtime_events\" WHERE sequence > $1 ORDER BY sequence LIMIT 256",
                [sequence.into()],
            )).await?;
            rows.into_iter().map(|row| Ok((
                row.try_get("", "sequence")?, row.try_get("", "source_id")?,
                RealtimeEvent {
                    id: row.try_get("", "id")?, event: row.try_get("", "event")?,
                    data: row.try_get("", "data")?, resource: row.try_get("", "resource")?,
                    record_id: row.try_get("", "record_id")?, actor_id: row.try_get("", "actor_id")?,
                    tenant_id: row.try_get("", "tenant_id")?, occurred_at: row.try_get("", "occurred_at")?,
                    resource_model: row.try_get("", "resource_model")?,
                },
            ))).collect()
        }
    }
}
