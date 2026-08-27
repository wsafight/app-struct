use crate::{ServiceHandle, ServiceHandles, StartupError};
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModulePhase {
    Starting,
    Started,
    Failed,
    Stopping,
    Stopped,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleEvent {
    pub module: String,
    pub phase: ModulePhase,
    pub detail: Option<String>,
}

pub trait ModuleObserver: Send + Sync {
    fn observe(&self, event: &ModuleEvent);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleDescriptor {
    pub name: &'static str,
    pub dependencies: &'static [&'static str],
}

impl ModuleDescriptor {
    #[must_use]
    pub const fn new(name: &'static str, dependencies: &'static [&'static str]) -> Self {
        Self { name, dependencies }
    }
}

#[async_trait]
pub trait ModuleStarter<Context>: Send {
    fn descriptor(&self) -> ModuleDescriptor;

    async fn start(
        self: Box<Self>,
        context: &mut Context,
    ) -> Result<Option<Box<dyn ServiceHandle>>, StartupError>;
}

pub struct ModulePlan<Context> {
    starters: Vec<Box<dyn ModuleStarter<Context>>>,
    observer: Option<Arc<dyn ModuleObserver>>,
}

impl<Context> Default for ModulePlan<Context> {
    fn default() -> Self {
        Self {
            starters: Vec::new(),
            observer: None,
        }
    }
}

impl<Context: Send> ModulePlan<Context> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, starter: impl ModuleStarter<Context> + 'static) {
        self.starters.push(Box::new(starter));
    }

    #[must_use]
    pub fn with_observer(mut self, observer: impl ModuleObserver + 'static) -> Self {
        self.observer = Some(Arc::new(observer));
        self
    }

    /// # Errors
    ///
    /// Returns the failing module error after reverse shutdown. Duplicate modules and dependencies
    /// that have not already started are rejected.
    pub async fn start(self, context: &mut Context) -> Result<ModuleRuntime, StartupError> {
        let mut runtime = ModuleRuntime::with_observer(self.observer);
        let mut descriptors = BTreeMap::new();
        for starter in self.starters {
            let descriptor = starter.descriptor();
            runtime.notify(descriptor.name, ModulePhase::Starting, None);
            if runtime.contains(descriptor.name) {
                let error = StartupError::configuration(
                    descriptor.name,
                    "module appears more than once in the runtime plan",
                );
                runtime.notify(
                    descriptor.name,
                    ModulePhase::Failed,
                    Some(error.to_string()),
                );
                return Err(rollback(error, descriptor.name, &descriptors, &mut runtime).await);
            }
            descriptors.insert(descriptor.name, descriptor.dependencies);
            if let Some(dependency) = descriptor
                .dependencies
                .iter()
                .find(|dependency| !runtime.contains(dependency))
            {
                let error = StartupError::dependency(descriptor.name, dependency);
                runtime.notify(
                    descriptor.name,
                    ModulePhase::Failed,
                    Some(error.to_string()),
                );
                return Err(rollback(error, descriptor.name, &descriptors, &mut runtime).await);
            }
            match starter.start(context).await {
                Ok(handle) => {
                    runtime.record_started(descriptor.name, handle);
                    runtime.notify(descriptor.name, ModulePhase::Started, None);
                }
                Err(error) => {
                    runtime.notify(
                        descriptor.name,
                        ModulePhase::Failed,
                        Some(error.to_string()),
                    );
                    return Err(rollback(error, descriptor.name, &descriptors, &mut runtime).await);
                }
            }
        }
        Ok(runtime)
    }
}

pub struct ModuleRuntime {
    started: Vec<String>,
    handles: ServiceHandles,
    observer: Option<Arc<dyn ModuleObserver>>,
}

impl Default for ModuleRuntime {
    fn default() -> Self {
        Self::with_observer(None)
    }
}

impl ModuleRuntime {
    #[must_use]
    pub fn started(&self) -> &[String] {
        &self.started
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.started.is_empty()
    }

    pub fn push_service(
        &mut self,
        module: impl Into<String>,
        handle: impl ServiceHandle + 'static,
    ) {
        self.started.push(module.into());
        self.handles.push(handle);
    }

    pub async fn shutdown_reverse(&mut self) -> Vec<String> {
        self.shutdown_with_phases(ModulePhase::Stopping, ModulePhase::Stopped)
            .await
    }

    pub async fn rollback_reverse(&mut self) -> Vec<String> {
        self.shutdown_with_phases(ModulePhase::RollingBack, ModulePhase::RolledBack)
            .await
    }

    async fn shutdown_with_phases(
        &mut self,
        before: ModulePhase,
        after: ModulePhase,
    ) -> Vec<String> {
        let modules = self.started.iter().rev().cloned().collect::<Vec<_>>();
        for module in &modules {
            self.notify(module, before, None);
        }
        let _ = self.handles.shutdown_reverse().await;
        self.started.clear();
        for module in &modules {
            self.notify(module, after, None);
        }
        modules
    }

    fn contains(&self, module: &str) -> bool {
        self.started.iter().any(|started| started == module)
    }

    fn record_started(&mut self, module: &str, handle: Option<Box<dyn ServiceHandle>>) {
        self.started.push(module.to_owned());
        if let Some(handle) = handle {
            self.handles.push_boxed(handle);
        }
    }

    fn with_observer(observer: Option<Arc<dyn ModuleObserver>>) -> Self {
        Self {
            started: Vec::new(),
            handles: ServiceHandles::default(),
            observer,
        }
    }

    fn notify(&self, module: &str, phase: ModulePhase, detail: Option<String>) {
        if let Some(observer) = &self.observer {
            observer.observe(&ModuleEvent {
                module: module.to_owned(),
                phase,
                detail,
            });
        }
    }
}

impl std::fmt::Debug for ModuleRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModuleRuntime")
            .field("started", &self.started)
            .field("service_handles", &self.handles.len())
            .field("observed", &self.observer.is_some())
            .finish()
    }
}

async fn rollback(
    error: StartupError,
    failed: &str,
    descriptors: &BTreeMap<&str, &[&str]>,
    runtime: &mut ModuleRuntime,
) -> StartupError {
    let chain = dependency_chain(failed, descriptors);
    let rolled_back = runtime.rollback_reverse().await;
    error.with_runtime_context(chain, rolled_back)
}

fn dependency_chain(target: &str, descriptors: &BTreeMap<&str, &[&str]>) -> Vec<String> {
    fn visit(
        module: &str,
        descriptors: &BTreeMap<&str, &[&str]>,
        visited: &mut BTreeSet<String>,
        ordered: &mut Vec<String>,
    ) {
        if !visited.insert(module.to_owned()) {
            return;
        }
        if let Some(dependencies) = descriptors.get(module) {
            for dependency in *dependencies {
                visit(dependency, descriptors, visited, ordered);
            }
        }
        ordered.push(module.to_owned());
    }

    let mut ordered = Vec::new();
    visit(target, descriptors, &mut BTreeSet::new(), &mut ordered);
    ordered
}
#[cfg(test)]
mod tests {
    use super::{
        ModuleDescriptor, ModuleEvent, ModuleObserver, ModulePhase, ModulePlan, ModuleStarter,
    };
    use crate::{ServiceHandle, StartupError};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct Starter {
        module: &'static str,
        dependencies: &'static [&'static str],
        fail: bool,
        events: Arc<Mutex<Vec<String>>>,
    }

    struct Handle {
        module: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    }

    struct Observer(Arc<Mutex<Vec<ModuleEvent>>>);

    impl ModuleObserver for Observer {
        fn observe(&self, event: &ModuleEvent) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    #[async_trait]
    impl ModuleStarter<()> for Starter {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor::new(self.module, self.dependencies)
        }

        async fn start(
            self: Box<Self>,
            _context: &mut (),
        ) -> Result<Option<Box<dyn ServiceHandle>>, StartupError> {
            self.events.lock().unwrap().push(self.module.to_owned());
            if self.fail {
                return Err(StartupError::configuration(self.module, "injected failure"));
            }
            Ok(Some(Box::new(Handle {
                module: self.module,
                events: Arc::clone(&self.events),
            })))
        }
    }

    #[async_trait]
    impl ServiceHandle for Handle {
        fn service(&self) -> &'static str {
            self.module
        }

        async fn shutdown(self: Box<Self>) {
            self.events
                .lock()
                .unwrap()
                .push(format!("stop:{}", self.module));
        }
    }

    fn starter(
        module: &'static str,
        dependencies: &'static [&'static str],
        fail: bool,
        events: &Arc<Mutex<Vec<String>>>,
    ) -> Starter {
        Starter {
            module,
            dependencies,
            fail,
            events: Arc::clone(events),
        }
    }

    #[tokio::test]
    async fn rolls_back_started_modules_when_startup_fails() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let lifecycle = Arc::new(Mutex::new(Vec::new()));
        let mut plan = ModulePlan::new().with_observer(Observer(Arc::clone(&lifecycle)));
        plan.push(starter("auth", &[], false, &events));
        plan.push(starter("mail", &["auth"], false, &events));
        plan.push(starter("jobs", &["mail"], true, &events));

        let error = plan.start(&mut ()).await.unwrap_err();
        assert_eq!(error.service(), "jobs");
        assert_eq!(error.dependency_chain(), ["auth", "mail", "jobs"]);
        assert_eq!(error.rolled_back(), ["mail", "auth"]);
        assert_eq!(
            *events.lock().unwrap(),
            ["auth", "mail", "jobs", "stop:mail", "stop:auth"]
        );
        let observed = lifecycle
            .lock()
            .unwrap()
            .iter()
            .map(|event| (event.module.clone(), event.phase))
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            [
                ("auth".to_owned(), ModulePhase::Starting),
                ("auth".to_owned(), ModulePhase::Started),
                ("mail".to_owned(), ModulePhase::Starting),
                ("mail".to_owned(), ModulePhase::Started),
                ("jobs".to_owned(), ModulePhase::Starting),
                ("jobs".to_owned(), ModulePhase::Failed),
                ("mail".to_owned(), ModulePhase::RollingBack),
                ("auth".to_owned(), ModulePhase::RollingBack),
                ("mail".to_owned(), ModulePhase::RolledBack),
                ("auth".to_owned(), ModulePhase::RolledBack),
            ]
        );
    }

    #[tokio::test]
    async fn rejects_out_of_order_dependencies_and_rolls_back() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut plan = ModulePlan::new();
        plan.push(starter("auth", &[], false, &events));
        plan.push(starter("tenant", &["rbac"], false, &events));

        let error = plan.start(&mut ()).await.unwrap_err();
        assert_eq!(error.service(), "tenant");
        assert_eq!(error.rolled_back(), ["auth"]);
        assert_eq!(*events.lock().unwrap(), ["auth", "stop:auth"]);
    }
}
