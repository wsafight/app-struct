//! Stable contracts shared by generated `AppStruct` backends.

mod lifecycle;
mod origin;
mod query;
mod resource;
mod supervisor;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::time::Duration;

/// Source files embedded into generated backend projects.
#[doc(hidden)]
pub mod __source {
    pub const LIB: &str = include_str!("lib.rs");
    pub const LIFECYCLE: &str = include_str!("lifecycle.rs");
    pub const ORIGIN: &str = include_str!("origin.rs");
    pub const QUERY: &str = include_str!("query.rs");
    pub const RESOURCE: &str = include_str!("resource.rs");
    pub const SUPERVISOR: &str = include_str!("supervisor.rs");
}

pub use lifecycle::{
    ModuleDescriptor, ModuleEvent, ModuleObserver, ModulePhase, ModulePlan, ModuleRuntime,
    ModuleStarter,
};
pub use origin::validate_browser_origin;
pub use query::{MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE, like_contains_pattern, list_page_is_valid};
pub use resource::{
    BulkDeleteInput, BulkFailure, BulkResult, BulkUpdateInput, CSV_EXPORT_PAGE_SIZE, CsvError,
    ListMeta, ListQuery, ListResponse, MAX_BULK_ITEMS, MAX_CSV_EXPORT_ROWS, MAX_CSV_IMPORT_ROWS,
    bulk_failure, bulk_request_size_is_valid, csv_escape, csv_json_value, decode_cursor,
    encode_cursor, parse_csv_rows, parse_revision_etag, revision_etag,
};
pub use supervisor::{
    BackgroundTaskExit, BackgroundTaskExitKind, BackgroundTaskObserver, SupervisedTaskHandle,
};

/// Version of the generated-backend/runtime contract.
pub const RUNTIME_API_VERSION: u32 = appstruct_contracts::RUNTIME_API.current;

pub const DEFAULT_SERVICE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

#[must_use]
pub const fn supports_runtime_api(version: u32) -> bool {
    appstruct_contracts::RUNTIME_API.supports(version)
}

pub type TenantId = uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: uuid::Uuid,
    pub email: String,
    pub roles: Vec<String>,
}

impl Actor {
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|candidate| candidate == role)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupError {
    service: String,
    message: String,
    dependency_chain: Vec<String>,
    rolled_back: Vec<String>,
    rollback_failures: Vec<ShutdownFailure>,
}

impl StartupError {
    #[must_use]
    pub fn configuration(service: impl Into<String>, error: impl fmt::Display) -> Self {
        Self {
            service: service.into(),
            message: error.to_string(),
            dependency_chain: Vec::new(),
            rolled_back: Vec::new(),
            rollback_failures: Vec::new(),
        }
    }

    #[must_use]
    pub fn dependency(service: impl Into<String>, dependency: impl fmt::Display) -> Self {
        Self::configuration(
            service,
            format_args!("runtime dependency `{dependency}` was not started"),
        )
    }

    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }

    #[must_use]
    pub fn dependency_chain(&self) -> &[String] {
        &self.dependency_chain
    }

    #[must_use]
    pub fn rolled_back(&self) -> &[String] {
        &self.rolled_back
    }

    #[must_use]
    pub fn rollback_failures(&self) -> &[ShutdownFailure] {
        &self.rollback_failures
    }

    #[must_use]
    pub fn with_runtime_context(
        mut self,
        dependency_chain: Vec<String>,
        rollback: ShutdownReport,
    ) -> Self {
        self.dependency_chain = dependency_chain;
        self.rolled_back = rollback.attempted;
        self.rollback_failures = rollback.failures;
        self
    }
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "failed to initialize {}: {}",
            self.service, self.message
        )?;
        if !self.dependency_chain.is_empty() {
            write!(
                formatter,
                "; dependency chain: {}",
                self.dependency_chain.join(" -> ")
            )?;
        }
        if !self.rolled_back.is_empty() {
            write!(formatter, "; rolled back: {}", self.rolled_back.join(", "))?;
        }
        if !self.rollback_failures.is_empty() {
            write!(
                formatter,
                "; rollback failures: {}",
                self.rollback_failures
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }
        Ok(())
    }
}

impl Error for StartupError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownError {
    message: String,
}

impl ShutdownError {
    #[must_use]
    pub fn new(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl fmt::Display for ShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ShutdownError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownFailureKind {
    Error,
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownFailure {
    pub service: String,
    pub kind: ShutdownFailureKind,
    pub message: String,
}

impl fmt::Display for ShutdownFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.service, self.message)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    pub attempted: Vec<String>,
    pub failures: Vec<ShutdownFailure>,
}

impl ShutdownReport {
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failures.is_empty()
    }
}

impl fmt::Display for ShutdownReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.failures.is_empty() {
            formatter.write_str("shutdown completed")
        } else {
            write!(
                formatter,
                "shutdown failed: {}",
                self.failures
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

#[async_trait]
pub trait ServiceHandle: Send {
    fn service(&self) -> &'static str;
    async fn shutdown(self: Box<Self>) -> Result<(), ShutdownError>;
}

#[derive(Default)]
pub struct ServiceHandles {
    handles: Vec<Box<dyn ServiceHandle>>,
}

impl ServiceHandles {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, handle: impl ServiceHandle + 'static) {
        self.handles.push(Box::new(handle));
    }

    pub fn push_boxed(&mut self, handle: Box<dyn ServiceHandle>) {
        self.handles.push(handle);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    pub async fn shutdown_reverse(&mut self) -> ShutdownReport {
        self.shutdown_reverse_with_timeout(DEFAULT_SERVICE_SHUTDOWN_TIMEOUT)
            .await
    }

    pub async fn shutdown_reverse_with_timeout(&mut self, timeout: Duration) -> ShutdownReport {
        let mut report = ShutdownReport {
            attempted: Vec::with_capacity(self.handles.len()),
            failures: Vec::new(),
        };
        while let Some(handle) = self.handles.pop() {
            let service = handle.service().to_owned();
            report.attempted.push(service.clone());
            match tokio::time::timeout(timeout, handle.shutdown()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => report.failures.push(ShutdownFailure {
                    service,
                    kind: ShutdownFailureKind::Error,
                    message: error.to_string(),
                }),
                Err(_) => report.failures.push(ShutdownFailure {
                    service,
                    kind: ShutdownFailureKind::Timeout,
                    message: format!("did not stop within {} ms", timeout.as_millis()),
                }),
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RUNTIME_API_VERSION, ServiceHandle, ServiceHandles, ShutdownError, ShutdownFailureKind,
        supports_runtime_api,
    };
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct RecordingHandle {
        service: &'static str,
        stopped: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl ServiceHandle for RecordingHandle {
        fn service(&self) -> &'static str {
            self.service
        }

        async fn shutdown(self: Box<Self>) -> Result<(), ShutdownError> {
            self.stopped.lock().unwrap().push(self.service);
            Ok(())
        }
    }

    #[tokio::test]
    async fn stops_services_in_reverse_startup_order() {
        let stopped = Arc::new(Mutex::new(Vec::new()));
        let mut handles = ServiceHandles::new();
        for service in ["auth", "jobs", "web"] {
            handles.push(RecordingHandle {
                service,
                stopped: Arc::clone(&stopped),
            });
        }
        let report = handles.shutdown_reverse().await;
        assert_eq!(report.attempted, ["web", "jobs", "auth"]);
        assert!(report.is_success());
        assert_eq!(*stopped.lock().unwrap(), ["web", "jobs", "auth"]);
        assert!(handles.is_empty());
    }

    struct FailingHandle {
        service: &'static str,
        hangs: bool,
    }

    #[async_trait]
    impl ServiceHandle for FailingHandle {
        fn service(&self) -> &'static str {
            self.service
        }

        async fn shutdown(self: Box<Self>) -> Result<(), ShutdownError> {
            if self.hangs {
                std::future::pending().await
            } else {
                Err(ShutdownError::new("injected shutdown error"))
            }
        }
    }

    #[tokio::test]
    async fn reports_shutdown_errors_and_timeouts_without_skipping_services() {
        let mut handles = ServiceHandles::new();
        handles.push(FailingHandle {
            service: "failure",
            hangs: false,
        });
        handles.push(FailingHandle {
            service: "timeout",
            hangs: true,
        });

        let report = handles
            .shutdown_reverse_with_timeout(std::time::Duration::from_millis(1))
            .await;
        assert_eq!(report.attempted, ["timeout", "failure"]);
        assert_eq!(report.failures.len(), 2);
        assert_eq!(report.failures[0].kind, ShutdownFailureKind::Timeout);
        assert_eq!(report.failures[1].kind, ShutdownFailureKind::Error);
    }

    #[test]
    fn runtime_api_compatibility_is_explicit() {
        assert!(supports_runtime_api(RUNTIME_API_VERSION));
        assert!(!supports_runtime_api(RUNTIME_API_VERSION + 1));
    }
}
