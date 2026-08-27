use super::render;
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

mod disabled;

use disabled::disabled_source;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.jobs.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/jobs.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let contract = contract_source();
    let queues = queue_source(ir);
    let enqueue = enqueue_source();
    let worker = worker_source(ir.jobs.poll_interval_ms, ir.jobs.lease_seconds);
    let persistence = persistence_source();
    let mail = (ir.mail.enabled).then(mail_source);
    render(quote! {
        use async_trait::async_trait;
        use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
        use serde::Serialize;
        use std::{
            fmt,
            sync::{Arc, atomic::{AtomicBool, Ordering}},
            time::Duration,
        };
        #contract
        #queues
        #enqueue
        #worker
        #persistence
        #mail
    })
}

fn contract_source() -> proc_macro2::TokenStream {
    quote! {
        #[derive(Clone, Debug)]
        pub struct Job {
            pub id: uuid::Uuid,
            pub queue: String,
            pub kind: String,
            pub payload: serde_json::Value,
            pub tenant_id: Option<uuid::Uuid>,
            pub attempts: i32,
            pub max_attempts: i32,
            pub backoff_seconds: i64,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct JobReceipt { pub id: uuid::Uuid, pub deduplicated: bool }
        #[derive(Clone, Debug)]
        pub struct JobHandlerError(pub String);
        impl fmt::Display for JobHandlerError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
        impl std::error::Error for JobHandlerError {}
        #[derive(Debug)]
        pub enum JobError {
            Disabled, UnknownQueue(String), InvalidInput(String),
            Serialization(String), Database(DbErr), LeaseLost,
        }
        impl fmt::Display for JobError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Disabled => formatter.write_str("jobs module is disabled"),
                    Self::UnknownQueue(name) => write!(formatter, "unknown job queue `{name}`"),
                    Self::InvalidInput(error) => write!(formatter, "invalid job input: {error}"),
                    Self::Serialization(error) => write!(formatter, "job serialization failed: {error}"),
                    Self::Database(error) => write!(formatter, "jobs database operation failed: {error}"),
                    Self::LeaseLost => formatter.write_str("job lease is no longer owned by this worker"),
                }
            }
        }
        impl std::error::Error for JobError {}
        impl From<DbErr> for JobError {
            fn from(error: DbErr) -> Self { Self::Database(error) }
        }
        #[async_trait]
        pub trait JobHandler: Send + Sync {
            async fn handle(&self, job: &Job) -> Result<(), JobHandlerError>;
        }
    }
}

fn queue_source(ir: &AppIr) -> proc_macro2::TokenStream {
    let queues = ir.jobs.queues.iter().map(|queue| {
        let name = &queue.name;
        let max_attempts = i32::try_from(queue.max_attempts).unwrap_or(100);
        let backoff_seconds = i64::try_from(queue.backoff_seconds).unwrap_or(3_600);
        quote! { #name => Some(QueueConfig { max_attempts: #max_attempts, backoff_seconds: #backoff_seconds }) }
    });
    quote! {
        #[derive(Clone, Copy)]
        struct QueueConfig { max_attempts: i32, backoff_seconds: i64 }
        fn queue_config(name: &str) -> Option<QueueConfig> {
            match name { #(#queues,)* _ => None }
        }
    }
}

fn enqueue_source() -> proc_macro2::TokenStream {
    quote! {
        pub(crate) async fn enqueue<C: ConnectionTrait, T: Serialize>(
            database: &C, queue: &str, kind: &str, payload: &T,
            idempotency_key: Option<&str>,
            run_at: Option<chrono::DateTime<chrono::Utc>>,
            tenant_id: Option<uuid::Uuid>,
        ) -> Result<JobReceipt, JobError> {
            let config = queue_config(queue)
                .ok_or_else(|| JobError::UnknownQueue(queue.to_owned()))?;
            if kind.trim().is_empty() || kind.len() > 120 {
                return Err(JobError::InvalidInput(
                    "kind must contain between 1 and 120 bytes".to_owned()
                ));
            }
            if idempotency_key.is_some_and(|key| key.is_empty() || key.len() > 200) {
                return Err(JobError::InvalidInput(
                    "idempotency key must contain between 1 and 200 bytes".to_owned()
                ));
            }
            let payload = serde_json::to_value(payload)
                .map_err(|error| JobError::Serialization(error.to_string()))?;
            let id = uuid::Uuid::now_v7();
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_jobs\" (id, queue, kind, payload, idempotency_key, tenant_id, status, attempts, max_attempts, backoff_seconds, run_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, 'queued', 0, $7, $8, COALESCE($9, CURRENT_TIMESTAMP), CURRENT_TIMESTAMP) ON CONFLICT (idempotency_key) DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key RETURNING id, (xmax <> 0) AS deduplicated",
                [
                    id.into(), queue.to_owned().into(), kind.to_owned().into(), payload.into(),
                    idempotency_key.map(str::to_owned).into(), tenant_id.into(),
                    config.max_attempts.into(), config.backoff_seconds.into(), run_at.into(),
                ],
            )).await?.ok_or_else(|| JobError::Database(DbErr::Custom(
                "job enqueue returned no row".to_owned()
            )))?;
            Ok(JobReceipt {
                id: row.try_get("", "id")?, deduplicated: row.try_get("", "deduplicated")?,
            })
        }
    }
}

fn worker_source(poll_interval_ms: u64, lease_seconds: u64) -> proc_macro2::TokenStream {
    let lease_seconds = i64::try_from(lease_seconds).unwrap_or(30);
    let lifecycle = worker_lifecycle_source();
    quote! {
        pub struct JobWorker {
            database: DatabaseConnection,
            handler: Arc<dyn JobHandler>,
            worker_id: String,
            kind: Option<String>,
        }
        impl JobWorker {
            pub fn new(database: DatabaseConnection, handler: Arc<dyn JobHandler>) -> Self {
                Self {
                    database, handler, worker_id: uuid::Uuid::now_v7().to_string(), kind: None,
                }
            }
            pub fn for_kind(
                database: DatabaseConnection, handler: Arc<dyn JobHandler>, kind: &str,
            ) -> Self {
                Self {
                    database, handler, worker_id: uuid::Uuid::now_v7().to_string(),
                    kind: Some(kind.to_owned()),
                }
            }
            pub async fn run_once(&self) -> Result<bool, JobError> {
                let Some(job) = claim(
                    &self.database, &self.worker_id, #lease_seconds, self.kind.as_deref(),
                ).await? else {
                    return Ok(false);
                };
                match self.handler.handle(&job).await {
                    Ok(()) => complete(&self.database, &self.worker_id, job.id).await?,
                    Err(error) => fail(&self.database, &self.worker_id, &job, &error.0).await?,
                }
                Ok(true)
            }
            pub fn spawn(self) -> JobWorkerHandle {
                self.spawn_inner(None)
            }
            pub(crate) fn spawn_with_health(
                self, health: crate::ApplicationHealth,
            ) -> JobWorkerHandle {
                self.spawn_inner(Some(health))
            }
            fn spawn_inner(
                self, health: Option<crate::ApplicationHealth>,
            ) -> JobWorkerHandle {
                let (shutdown, mut receiver) = tokio::sync::watch::channel(false);
                let expected_shutdown = Arc::new(AtomicBool::new(false));
                let task_expected_shutdown = Arc::clone(&expected_shutdown);
                let task = tokio::spawn(async move {
                    let _exit = WorkerExitGuard { health, expected_shutdown: task_expected_shutdown };
                    loop {
                        if *receiver.borrow() { break; }
                        match self.run_once().await {
                            Ok(true) => continue,
                            Ok(false) => {}
                            Err(error) => tracing::error!(%error, "job worker iteration failed"),
                        }
                        tokio::select! {
                            () = tokio::time::sleep(Duration::from_millis(#poll_interval_ms)) => {}
                            result = receiver.changed() => if result.is_err() { break; }
                        }
                    }
                });
                JobWorkerHandle { shutdown, task, expected_shutdown }
            }
        }
        #lifecycle
    }
}

fn worker_lifecycle_source() -> proc_macro2::TokenStream {
    quote! {
        struct WorkerExitGuard {
            health: Option<crate::ApplicationHealth>,
            expected_shutdown: Arc<AtomicBool>,
        }
        impl Drop for WorkerExitGuard {
            fn drop(&mut self) {
                if !self.expected_shutdown.load(Ordering::Acquire) {
                    if let Some(health) = &self.health {
                        health.mark_failed("job worker exited unexpectedly");
                    }
                }
            }
        }
        pub struct JobWorkerHandle {
            shutdown: tokio::sync::watch::Sender<bool>,
            task: tokio::task::JoinHandle<()>,
            expected_shutdown: Arc<AtomicBool>,
        }
        impl JobWorkerHandle {
            pub async fn shutdown(self) -> Result<(), appstruct_runtime::ShutdownError> {
                self.expected_shutdown.store(true, Ordering::Release);
                self.shutdown.send(true).map_err(appstruct_runtime::ShutdownError::new)?;
                self.task.await.map_err(appstruct_runtime::ShutdownError::new)
            }
        }
        #[async_trait]
        impl appstruct_runtime::ServiceHandle for JobWorkerHandle {
            fn service(&self) -> &'static str { "appstruct/jobs" }
            async fn shutdown(
                self: Box<Self>,
            ) -> Result<(), appstruct_runtime::ShutdownError> {
                JobWorkerHandle::shutdown(*self).await
            }
        }
    }
}

fn persistence_source() -> proc_macro2::TokenStream {
    quote! {
        async fn claim(
            database: &DatabaseConnection, worker_id: &str, lease_seconds: i64,
            kind: Option<&str>,
        ) -> Result<Option<Job>, JobError> {
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "WITH candidate AS (SELECT id FROM \"_appstruct_jobs\" WHERE ((status = 'queued' AND run_at <= CURRENT_TIMESTAMP) OR (status = 'running' AND locked_until <= CURRENT_TIMESTAMP)) AND ($3::text IS NULL OR kind = $3) ORDER BY run_at, id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE \"_appstruct_jobs\" AS job SET status = 'running', attempts = attempts + 1, locked_by = $1, locked_until = CURRENT_TIMESTAMP + ($2 * INTERVAL '1 second') FROM candidate WHERE job.id = candidate.id RETURNING job.id, job.queue, job.kind, job.payload, job.tenant_id, job.attempts, job.max_attempts, job.backoff_seconds",
                [
                    worker_id.to_owned().into(), lease_seconds.into(),
                    kind.map(str::to_owned).into(),
                ],
            )).await?;
            row.map(job_from_row).transpose().map_err(JobError::from)
        }
        fn job_from_row(row: sea_orm::QueryResult) -> Result<Job, DbErr> {
            Ok(Job {
                id: row.try_get("", "id")?, queue: row.try_get("", "queue")?,
                kind: row.try_get("", "kind")?, payload: row.try_get("", "payload")?,
                tenant_id: row.try_get("", "tenant_id")?, attempts: row.try_get("", "attempts")?,
                max_attempts: row.try_get("", "max_attempts")?,
                backoff_seconds: row.try_get("", "backoff_seconds")?,
            })
        }
        async fn complete(
            database: &DatabaseConnection, worker_id: &str, id: uuid::Uuid,
        ) -> Result<(), JobError> {
            let result = database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_jobs\" SET status = 'succeeded', completed_at = CURRENT_TIMESTAMP, locked_by = NULL, locked_until = NULL, last_error = NULL WHERE id = $1 AND status = 'running' AND locked_by = $2",
                [id.into(), worker_id.to_owned().into()],
            )).await?;
            if result.rows_affected() == 1 { Ok(()) } else { Err(JobError::LeaseLost) }
        }
        async fn fail(
            database: &DatabaseConnection, worker_id: &str, job: &Job, error: &str,
        ) -> Result<(), JobError> {
            let terminal = job.attempts >= job.max_attempts;
            let status = if terminal { "dead" } else { "queued" };
            let exponent = u32::try_from((job.attempts - 1).clamp(0, 30)).unwrap_or(0);
            let delay = job.backoff_seconds.saturating_mul(1_i64 << exponent).min(3_600);
            let error = error.chars().take(2_000).collect::<String>();
            let result = database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_jobs\" SET status = $3, last_error = $4, locked_by = NULL, locked_until = NULL, run_at = CASE WHEN $3 = 'queued' THEN CURRENT_TIMESTAMP + ($5 * INTERVAL '1 second') ELSE run_at END, completed_at = CASE WHEN $3 = 'dead' THEN CURRENT_TIMESTAMP ELSE NULL END WHERE id = $1 AND status = 'running' AND locked_by = $2",
                [
                    job.id.into(), worker_id.to_owned().into(), status.to_owned().into(),
                    error.into(), delay.into(),
                ],
            )).await?;
            if result.rows_affected() == 1 { Ok(()) } else { Err(JobError::LeaseLost) }
        }
    }
}

fn mail_source() -> proc_macro2::TokenStream {
    quote! {
        #[derive(Clone, Debug, Serialize, serde::Deserialize)]
        pub struct MailJobPayload {
            pub template: String,
            pub recipient: String,
            pub variables: std::collections::BTreeMap<String, String>,
        }

        pub struct MailJobHandler { mail: crate::MailState }

        impl MailJobHandler {
            pub fn new(mail: crate::MailState) -> Self { Self { mail } }
        }

        #[async_trait]
        impl JobHandler for MailJobHandler {
            async fn handle(&self, job: &Job) -> Result<(), JobHandlerError> {
                if job.kind != "mail.send" {
                    return Err(JobHandlerError(format!(
                        "mail handler does not support job kind `{}`", job.kind
                    )));
                }
                let payload: MailJobPayload = serde_json::from_value(job.payload.clone())
                    .map_err(|error| JobHandlerError(error.to_string()))?;
                self.mail.send_template(
                    &payload.template, &payload.recipient, &payload.variables, job.tenant_id,
                ).await.map(|_| ()).map_err(|error| JobHandlerError(error.to_string()))
            }
        }
    }
}
