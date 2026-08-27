use super::{
    ModuleDescriptor, ModuleEvent, ModuleObserver, ModulePhase, ModulePlan, ModuleStarter,
};
use crate::{ServiceHandle, ShutdownError, StartupError};
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

    async fn shutdown(self: Box<Self>) -> Result<(), ShutdownError> {
        self.events
            .lock()
            .unwrap()
            .push(format!("stop:{}", self.module));
        Ok(())
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

struct PanickingObserver;

impl ModuleObserver for PanickingObserver {
    fn observe(&self, _event: &ModuleEvent) {
        panic!("injected observer panic");
    }
}

#[tokio::test]
async fn observer_panics_do_not_interrupt_module_lifecycle() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut plan = ModulePlan::new().with_observer(PanickingObserver);
    plan.push(starter("auth", &[], false, &events));

    let mut runtime = plan.start(&mut ()).await.unwrap();
    let report = runtime.shutdown_reverse().await;
    assert!(report.is_success());
    assert_eq!(report.attempted, ["auth"]);
}
