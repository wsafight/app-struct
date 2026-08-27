use crate::{ServiceHandle, ShutdownError};
use async_trait::async_trait;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use tokio::{
    sync::watch,
    task::{JoinError, JoinHandle},
};

const RUNNING: u8 = 0;
const SHUTTING_DOWN: u8 = 1;
const EXITED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundTaskExitKind {
    Completed,
    Failed,
    Panicked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundTaskExit {
    pub service: &'static str,
    pub kind: BackgroundTaskExitKind,
    pub message: Option<String>,
}

pub trait BackgroundTaskObserver: Send + Sync {
    fn exited(&self, event: &BackgroundTaskExit);
}

pub struct SupervisedTaskHandle {
    service: &'static str,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<Result<(), ShutdownError>>,
    state: Arc<AtomicU8>,
}

impl SupervisedTaskHandle {
    #[must_use]
    pub fn spawn<F, Fut, O>(service: &'static str, observer: O, run: F) -> Self
    where
        F: FnOnce(watch::Receiver<bool>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ShutdownError>> + Send + 'static,
        O: BackgroundTaskObserver + 'static,
    {
        let (shutdown, receiver) = watch::channel(false);
        let state = Arc::new(AtomicU8::new(RUNNING));
        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            let outcome = task_outcome(tokio::spawn(run(receiver)).await);
            if task_state
                .compare_exchange(RUNNING, EXITED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let event = BackgroundTaskExit {
                    service,
                    kind: outcome.kind,
                    message: outcome.message,
                };
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    observer.exited(&event);
                }));
            }
            outcome.result
        });
        Self {
            service,
            shutdown,
            task,
            state,
        }
    }

    /// Request shutdown and wait for the supervised task to finish.
    ///
    /// # Errors
    ///
    /// Returns an error when the task reports a failure, panics, or cannot be joined.
    pub async fn shutdown(self) -> Result<(), ShutdownError> {
        let Self {
            shutdown,
            task,
            state,
            ..
        } = self;
        let _ = state.compare_exchange(RUNNING, SHUTTING_DOWN, Ordering::AcqRel, Ordering::Acquire);
        let _ = shutdown.send(true);
        task.await.map_err(ShutdownError::new)?
    }
}

#[async_trait]
impl ServiceHandle for SupervisedTaskHandle {
    fn service(&self) -> &'static str {
        self.service
    }

    async fn shutdown(self: Box<Self>) -> Result<(), ShutdownError> {
        SupervisedTaskHandle::shutdown(*self).await
    }
}

struct TaskOutcome {
    result: Result<(), ShutdownError>,
    kind: BackgroundTaskExitKind,
    message: Option<String>,
}

fn task_outcome(result: Result<Result<(), ShutdownError>, JoinError>) -> TaskOutcome {
    match result {
        Ok(Ok(())) => TaskOutcome {
            result: Ok(()),
            kind: BackgroundTaskExitKind::Completed,
            message: None,
        },
        Ok(Err(error)) => {
            let message = error.to_string();
            TaskOutcome {
                result: Err(error),
                kind: BackgroundTaskExitKind::Failed,
                message: Some(message),
            }
        }
        Err(error) => {
            let kind = if error.is_panic() {
                BackgroundTaskExitKind::Panicked
            } else {
                BackgroundTaskExitKind::Failed
            };
            let message = error.to_string();
            TaskOutcome {
                result: Err(ShutdownError::new(&message)),
                kind,
                message: Some(message),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundTaskExit, BackgroundTaskExitKind, BackgroundTaskObserver, SupervisedTaskHandle,
    };
    use crate::{ServiceHandle, ShutdownError};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Clone, Default)]
    struct RecordingObserver(Arc<Mutex<Vec<BackgroundTaskExit>>>);

    impl BackgroundTaskObserver for RecordingObserver {
        fn exited(&self, event: &BackgroundTaskExit) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    impl RecordingObserver {
        async fn wait_for_exit(&self) -> BackgroundTaskExit {
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                loop {
                    if let Some(event) = self.0.lock().unwrap().first().cloned() {
                        return event;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap()
        }
    }

    #[tokio::test]
    async fn observes_unexpected_completion() {
        let observer = RecordingObserver::default();
        let handle =
            SupervisedTaskHandle::spawn("completed", observer.clone(), |_| async { Ok(()) });
        let event = observer.wait_for_exit().await;
        assert_eq!(event.kind, BackgroundTaskExitKind::Completed);
        assert_eq!(event.service, "completed");
        assert!(event.message.is_none());
        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn observes_task_errors_and_returns_them_from_shutdown() {
        let observer = RecordingObserver::default();
        let handle = SupervisedTaskHandle::spawn("failed", observer.clone(), |_| async {
            Err(ShutdownError::new("injected task error"))
        });
        let event = observer.wait_for_exit().await;
        assert_eq!(event.kind, BackgroundTaskExitKind::Failed);
        assert_eq!(event.message.as_deref(), Some("injected task error"));
        assert_eq!(
            Box::new(handle).shutdown().await.unwrap_err().to_string(),
            "injected task error"
        );
    }

    #[tokio::test]
    async fn observes_panics_and_returns_them_from_shutdown() {
        let observer = RecordingObserver::default();
        let handle = SupervisedTaskHandle::spawn("panicked", observer.clone(), |_| async {
            panic!("injected task panic");
        });
        let event = observer.wait_for_exit().await;
        assert_eq!(event.kind, BackgroundTaskExitKind::Panicked);
        assert!(event.message.unwrap().contains("injected task panic"));
        assert!(handle.shutdown().await.is_err());
    }

    #[tokio::test]
    async fn expected_shutdown_is_not_observed_as_an_exit() {
        let observer = RecordingObserver::default();
        let handle =
            SupervisedTaskHandle::spawn("stopped", observer.clone(), |mut shutdown| async move {
                shutdown.changed().await.map_err(ShutdownError::new)?;
                Ok(())
            });
        handle.shutdown().await.unwrap();
        assert!(observer.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn observer_panics_do_not_change_the_task_result() {
        struct PanickingObserver(Arc<AtomicBool>);
        impl BackgroundTaskObserver for PanickingObserver {
            fn exited(&self, _: &BackgroundTaskExit) {
                self.0.store(true, Ordering::Release);
                panic!("injected observer panic");
            }
        }

        let observed = Arc::new(AtomicBool::new(false));
        let handle = SupervisedTaskHandle::spawn(
            "observed",
            PanickingObserver(Arc::clone(&observed)),
            |_| async { Ok(()) },
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !observed.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        handle.shutdown().await.unwrap();
    }
}
