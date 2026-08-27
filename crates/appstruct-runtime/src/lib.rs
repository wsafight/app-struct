//! Stable contracts shared by generated `AppStruct` backends.

mod lifecycle;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

pub use lifecycle::{
    ModuleDescriptor, ModuleEvent, ModuleObserver, ModulePhase, ModulePlan, ModuleRuntime,
    ModuleStarter,
};

/// Version of the generated-backend/runtime contract.
pub const RUNTIME_API_VERSION: u32 = 1;

#[must_use]
pub const fn supports_runtime_api(version: u32) -> bool {
    version == RUNTIME_API_VERSION
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
}

impl StartupError {
    #[must_use]
    pub fn configuration(service: impl Into<String>, error: impl fmt::Display) -> Self {
        Self {
            service: service.into(),
            message: error.to_string(),
            dependency_chain: Vec::new(),
            rolled_back: Vec::new(),
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
    pub fn with_runtime_context(
        mut self,
        dependency_chain: Vec<String>,
        rolled_back: Vec<String>,
    ) -> Self {
        self.dependency_chain = dependency_chain;
        self.rolled_back = rolled_back;
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
        Ok(())
    }
}

impl Error for StartupError {}

#[async_trait]
pub trait ServiceHandle: Send {
    fn service(&self) -> &'static str;
    async fn shutdown(self: Box<Self>);
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

    pub async fn shutdown_reverse(&mut self) -> Vec<&'static str> {
        let mut stopped = Vec::with_capacity(self.handles.len());
        while let Some(handle) = self.handles.pop() {
            stopped.push(handle.service());
            handle.shutdown().await;
        }
        stopped
    }
}

#[cfg(test)]
mod tests {
    use super::{RUNTIME_API_VERSION, ServiceHandle, ServiceHandles, supports_runtime_api};
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

        async fn shutdown(self: Box<Self>) {
            self.stopped.lock().unwrap().push(self.service);
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
        let services = handles.shutdown_reverse().await;
        assert_eq!(services, ["web", "jobs", "auth"]);
        assert_eq!(*stopped.lock().unwrap(), ["web", "jobs", "auth"]);
        assert!(handles.is_empty());
    }

    #[test]
    fn runtime_api_compatibility_is_explicit() {
        assert!(supports_runtime_api(RUNTIME_API_VERSION));
        assert!(!supports_runtime_api(RUNTIME_API_VERSION + 1));
    }
}
