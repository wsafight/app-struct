use super::render;
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.webhooks.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/webhooks.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

#[allow(clippy::too_many_lines)]
fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let endpoints = endpoint_source(ir);
    let poll_interval = ir.webhooks.poll_interval_ms;
    let connect_timeout = ir.webhooks.connect_timeout_ms;
    let read_timeout = ir.webhooks.read_timeout_ms;
    let request_timeout = ir.webhooks.request_timeout_ms;
    render(quote! {
        use hmac::{Hmac, Mac};
        use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
        use serde::Serialize;
        use sha2::Sha256;
        use std::{env, fmt, time::Duration};

        #endpoints

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct WebhookReceipt { pub ids: Vec<uuid::Uuid> }

        #[derive(Debug)]
        pub enum WebhookError {
            Disabled, InvalidInput(String), Serialization(String),
            Configuration(String), Delivery(String), Database(DbErr), LeaseLost,
        }
        impl fmt::Display for WebhookError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Disabled => formatter.write_str("webhooks module is disabled"),
                    Self::InvalidInput(error) => write!(formatter, "invalid webhook input: {error}"),
                    Self::Serialization(error) => write!(formatter, "webhook serialization failed: {error}"),
                    Self::Configuration(error) => write!(formatter, "webhook configuration failed: {error}"),
                    Self::Delivery(error) => write!(formatter, "webhook delivery failed: {error}"),
                    Self::Database(error) => write!(formatter, "webhook database operation failed: {error}"),
                    Self::LeaseLost => formatter.write_str("webhook delivery lease was lost"),
                }
            }
        }
        impl std::error::Error for WebhookError {}
        impl From<DbErr> for WebhookError {
            fn from(error: DbErr) -> Self { Self::Database(error) }
        }

        pub(crate) async fn publish<C: ConnectionTrait, T: Serialize>(
            database: &C, event: &str, payload: &T, idempotency_key: Option<&str>,
            tenant_id: Option<uuid::Uuid>,
        ) -> Result<WebhookReceipt, WebhookError> {
            if event.is_empty() || event.len() > 120 {
                return Err(WebhookError::InvalidInput(
                    "event must contain between 1 and 120 bytes".to_owned(),
                ));
            }
            if idempotency_key.is_some_and(|key| key.is_empty() || key.len() > 180) {
                return Err(WebhookError::InvalidInput(
                    "idempotency key must contain between 1 and 180 bytes".to_owned(),
                ));
            }
            let payload = serde_json::to_value(payload)
                .map_err(|error| WebhookError::Serialization(error.to_string()))?;
            let mut ids = Vec::new();
            for endpoint in endpoint_configs().iter().filter(|endpoint| endpoint.accepts(event)) {
                let id = uuid::Uuid::now_v7();
                let key = idempotency_key.map(|key| format!("{}:{key}", endpoint.name));
                let row = database.query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "INSERT INTO \"_appstruct_webhook_deliveries\" (id, endpoint, event, payload, idempotency_key, tenant_id, status, attempts, max_attempts, backoff_seconds, next_attempt_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, 'pending', 0, $7, $8, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (idempotency_key) DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key RETURNING id",
                    [
                        id.into(), endpoint.name.to_owned().into(), event.to_owned().into(),
                        payload.clone().into(), key.into(), tenant_id.into(),
                        endpoint.max_attempts.into(), endpoint.backoff_seconds.into(),
                    ],
                )).await?.ok_or_else(|| WebhookError::Database(DbErr::Custom(
                    "webhook publish returned no row".to_owned(),
                )))?;
                ids.push(row.try_get("", "id")?);
            }
            Ok(WebhookReceipt { ids })
        }

        struct Delivery {
            id: uuid::Uuid, endpoint: String, event: String, payload: serde_json::Value,
            attempts: i32, max_attempts: i32, backoff_seconds: i64,
        }

        pub struct WebhookWorker {
            database: DatabaseConnection, client: reqwest::Client, worker_id: String,
        }
        impl WebhookWorker {
            pub fn new(database: DatabaseConnection) -> Self {
                let client = reqwest::Client::builder()
                    .connect_timeout(Duration::from_millis(#connect_timeout))
                    .read_timeout(Duration::from_millis(#read_timeout))
                    .timeout(Duration::from_millis(#request_timeout))
                    .build()
                    .expect("static webhook HTTP client configuration is valid");
                Self { database, client, worker_id: uuid::Uuid::now_v7().to_string() }
            }
            pub async fn run_once(&self) -> Result<bool, WebhookError> {
                let Some(delivery) = claim(&self.database, &self.worker_id).await? else {
                    return Ok(false);
                };
                let result = self.deliver(&delivery).await;
                match result {
                    Ok(status) => complete(&self.database, &self.worker_id, delivery.id, status).await?,
                    Err(error) => fail(&self.database, &self.worker_id, &delivery, &error.to_string()).await?,
                }
                Ok(true)
            }
            async fn deliver(&self, delivery: &Delivery) -> Result<i32, WebhookError> {
                let endpoint = endpoint_config(&delivery.endpoint).ok_or_else(|| {
                    WebhookError::Configuration(format!("unknown endpoint `{}`", delivery.endpoint))
                })?;
                let secret = env::var(endpoint.secret_env).map_err(|_| {
                    WebhookError::Configuration(format!("{} is required", endpoint.secret_env))
                })?;
                let timestamp = chrono::Utc::now().timestamp().to_string();
                let body = serde_json::to_vec(&delivery.payload)
                    .map_err(|error| WebhookError::Serialization(error.to_string()))?;
                let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
                    .map_err(|error| WebhookError::Configuration(error.to_string()))?;
                mac.update(timestamp.as_bytes());
                mac.update(b".");
                mac.update(&body);
                let signature = mac.finalize().into_bytes().iter()
                    .map(|byte| format!("{byte:02x}")).collect::<String>();
                let response = self.client.post(endpoint.url)
                    .header("content-type", "application/json")
                    .header("x-appstruct-delivery", delivery.id.to_string())
                    .header("x-appstruct-event", &delivery.event)
                    .header("x-appstruct-timestamp", timestamp)
                    .header("x-appstruct-signature", format!("v1={signature}"))
                    .body(body).send().await
                    .map_err(|error| {
                        if error.is_timeout() {
                            WebhookError::Delivery(format!("request timed out: {error}"))
                        } else {
                            WebhookError::Delivery(error.to_string())
                        }
                    })?;
                let status = i32::from(response.status().as_u16());
                if response.status().is_success() { Ok(status) } else {
                    Err(WebhookError::Delivery(format!("endpoint returned HTTP {status}")))
                }
            }
            pub(crate) fn spawn(self) -> WebhookWorkerHandle {
                let task = appstruct_runtime::SupervisedTaskHandle::spawn(
                    "appstruct/webhooks", WebhookWorkerObserver, move |mut receiver| async move {
                        loop {
                            if *receiver.borrow() { break; }
                            match self.run_once().await {
                                Ok(true) => continue,
                                Ok(false) => {}
                                Err(error) => tracing::error!(%error, "webhook worker iteration failed"),
                            }
                            tokio::select! {
                                () = tokio::time::sleep(Duration::from_millis(#poll_interval)) => {}
                                result = receiver.changed() => if result.is_err() { break; }
                            }
                        }
                        Ok(())
                    },
                );
                WebhookWorkerHandle { task }
            }
        }

        struct WebhookWorkerObserver;
        impl appstruct_runtime::BackgroundTaskObserver for WebhookWorkerObserver {
            fn exited(&self, exit: &appstruct_runtime::BackgroundTaskExit) {
                tracing::error!(
                    kind = ?exit.kind,
                    detail = exit.message.as_deref().unwrap_or("task completed"),
                    "webhook worker stopped unexpectedly",
                );
            }
        }
        pub struct WebhookWorkerHandle { task: appstruct_runtime::SupervisedTaskHandle }
        #[async_trait::async_trait]
        impl appstruct_runtime::ServiceHandle for WebhookWorkerHandle {
            fn service(&self) -> &'static str { "appstruct/webhooks" }
            async fn shutdown(self: Box<Self>) -> Result<(), appstruct_runtime::ShutdownError> {
                self.task.shutdown().await
            }
        }

        async fn claim(database: &DatabaseConnection, worker_id: &str) -> Result<Option<Delivery>, WebhookError> {
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "WITH candidate AS (SELECT id FROM \"_appstruct_webhook_deliveries\" WHERE ((status = 'pending' AND next_attempt_at <= CURRENT_TIMESTAMP) OR (status = 'delivering' AND locked_until <= CURRENT_TIMESTAMP)) ORDER BY next_attempt_at, id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE \"_appstruct_webhook_deliveries\" AS delivery SET status = 'delivering', attempts = attempts + 1, locked_by = $1, locked_until = CURRENT_TIMESTAMP + INTERVAL '30 seconds' FROM candidate WHERE delivery.id = candidate.id RETURNING delivery.id, delivery.endpoint, delivery.event, delivery.payload, delivery.attempts, delivery.max_attempts, delivery.backoff_seconds",
                [worker_id.to_owned().into()],
            )).await?;
            row.map(delivery_from_row).transpose().map_err(WebhookError::from)
        }
        fn delivery_from_row(row: sea_orm::QueryResult) -> Result<Delivery, DbErr> {
            Ok(Delivery {
                id: row.try_get("", "id")?, endpoint: row.try_get("", "endpoint")?,
                event: row.try_get("", "event")?, payload: row.try_get("", "payload")?,
                attempts: row.try_get("", "attempts")?, max_attempts: row.try_get("", "max_attempts")?,
                backoff_seconds: row.try_get("", "backoff_seconds")?,
            })
        }
        async fn complete(database: &DatabaseConnection, worker_id: &str, id: uuid::Uuid, status: i32) -> Result<(), WebhookError> {
            let result = database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_webhook_deliveries\" SET status = 'succeeded', response_status = $3, completed_at = CURRENT_TIMESTAMP, locked_by = NULL, locked_until = NULL, last_error = NULL WHERE id = $1 AND status = 'delivering' AND locked_by = $2",
                [id.into(), worker_id.to_owned().into(), status.into()],
            )).await?;
            if result.rows_affected() == 1 { Ok(()) } else { Err(WebhookError::LeaseLost) }
        }
        async fn fail(database: &DatabaseConnection, worker_id: &str, delivery: &Delivery, error: &str) -> Result<(), WebhookError> {
            let terminal = delivery.attempts >= delivery.max_attempts;
            let status = if terminal { "dead" } else { "pending" };
            let exponent = u32::try_from((delivery.attempts - 1).clamp(0, 30)).unwrap_or(0);
            let delay = delivery.backoff_seconds.saturating_mul(1_i64 << exponent).min(3_600);
            let result = database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_webhook_deliveries\" SET status = $3, last_error = $4, locked_by = NULL, locked_until = NULL, next_attempt_at = CASE WHEN $3 = 'pending' THEN CURRENT_TIMESTAMP + ($5 * INTERVAL '1 second') ELSE next_attempt_at END, completed_at = CASE WHEN $3 = 'dead' THEN CURRENT_TIMESTAMP ELSE NULL END WHERE id = $1 AND status = 'delivering' AND locked_by = $2",
                [delivery.id.into(), worker_id.to_owned().into(), status.to_owned().into(), error.chars().take(2_000).collect::<String>().into(), delay.into()],
            )).await?;
            if result.rows_affected() == 1 { Ok(()) } else { Err(WebhookError::LeaseLost) }
        }
    })
}

fn endpoint_source(ir: &AppIr) -> proc_macro2::TokenStream {
    let definitions = ir.webhooks.endpoints.iter().map(|endpoint| {
        let name = &endpoint.name;
        let url = &endpoint.url;
        let secret_env = &endpoint.secret_env;
        let events = &endpoint.events;
        let max_attempts = i32::try_from(endpoint.max_attempts).unwrap_or(100);
        let backoff_seconds = i64::try_from(endpoint.backoff_seconds).unwrap_or(3_600);
        quote! { EndpointConfig { name: #name, url: #url, secret_env: #secret_env, events: &[#(#events),*], max_attempts: #max_attempts, backoff_seconds: #backoff_seconds } }
    });
    quote! {
        #[derive(Clone, Copy)]
        struct EndpointConfig {
            name: &'static str, url: &'static str, secret_env: &'static str,
            events: &'static [&'static str], max_attempts: i32, backoff_seconds: i64,
        }
        impl EndpointConfig {
            fn accepts(self, event: &str) -> bool {
                self.events.iter().any(|candidate| *candidate == "*" || *candidate == event)
            }
        }
        fn endpoint_configs() -> &'static [EndpointConfig] { &[#(#definitions),*] }
        fn endpoint_config(name: &str) -> Option<EndpointConfig> {
            endpoint_configs().iter().copied().find(|endpoint| endpoint.name == name)
        }
    }
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use sea_orm::{ConnectionTrait, DatabaseConnection};
        use serde::Serialize;
        use std::fmt;
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct WebhookReceipt { pub ids: Vec<uuid::Uuid> }
        #[derive(Debug)] pub enum WebhookError { Disabled }
        impl fmt::Display for WebhookError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("webhooks module is disabled")
            }
        }
        impl std::error::Error for WebhookError {}
        pub struct WebhookWorker;
        impl WebhookWorker { pub fn new(_database: DatabaseConnection) -> Self { Self } }
        pub struct WebhookWorkerHandle;
        #[async_trait::async_trait]
        impl appstruct_runtime::ServiceHandle for WebhookWorkerHandle {
            fn service(&self) -> &'static str { "appstruct/webhooks" }
            async fn shutdown(self: Box<Self>) -> Result<(), appstruct_runtime::ShutdownError> { Ok(()) }
        }
        pub(crate) async fn publish<C: ConnectionTrait, T: Serialize>(
            _database: &C, _event: &str, _payload: &T, _idempotency_key: Option<&str>,
            _tenant_id: Option<uuid::Uuid>,
        ) -> Result<WebhookReceipt, WebhookError> { Err(WebhookError::Disabled) }
    })
}
