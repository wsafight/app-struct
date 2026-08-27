use super::{CodegenError, render};
use quote::quote;

pub(super) fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use async_trait::async_trait;
        use sea_orm::{ConnectionTrait, DatabaseConnection};
        use serde::Serialize;
        use std::{fmt, sync::Arc};
        #[derive(Clone, Debug)] pub struct Job;
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct JobReceipt { pub id: uuid::Uuid, pub deduplicated: bool }
        #[derive(Clone, Debug)] pub struct JobHandlerError(pub String);
        #[derive(Debug)] pub enum JobError { Disabled }
        impl fmt::Display for JobError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("jobs module is disabled")
            }
        }
        impl std::error::Error for JobError {}
        #[async_trait]
        pub trait JobHandler: Send + Sync {
            async fn handle(&self, job: &Job) -> Result<(), JobHandlerError>;
        }
        pub struct JobWorker;
        impl JobWorker {
            pub fn new(_database: DatabaseConnection, _handler: Arc<dyn JobHandler>) -> Self { Self }
            pub fn for_kind(
                _database: DatabaseConnection, _handler: Arc<dyn JobHandler>, _kind: &str,
            ) -> Self { Self }
            pub async fn run_once(&self) -> Result<bool, JobError> { Err(JobError::Disabled) }
            pub fn spawn(self) -> JobWorkerHandle { JobWorkerHandle }
        }
        pub struct JobWorkerHandle;
        impl JobWorkerHandle {
            pub async fn shutdown(self) -> Result<(), appstruct_runtime::ShutdownError> { Ok(()) }
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
        pub(crate) async fn enqueue<C: ConnectionTrait, T: Serialize>(
            _database: &C, _queue: &str, _kind: &str, _payload: &T,
            _idempotency_key: Option<&str>, _run_at: Option<chrono::DateTime<chrono::Utc>>,
            _tenant_id: Option<uuid::Uuid>,
        ) -> Result<JobReceipt, JobError> { Err(JobError::Disabled) }
    })
}
