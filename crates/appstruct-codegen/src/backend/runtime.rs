use crate::CodegenError;
use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source(ir: &AppIr, routes: &[TokenStream]) -> Result<TokenStream, CodegenError> {
    let contract = contract_source();
    let application = application_source();
    let routing = router_source(routes);
    let start_worker = start_worker(ir);
    let start_webhooks = start_webhook_worker(ir);
    let startup = super::startup::source(ir)?;
    let lifecycle = lifecycle_source();
    Ok(quote! {
        use axum::{
            Router, extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse,
            routing::get,
        };
        use sea_orm::DatabaseConnection;
        use std::{
            future::IntoFuture,
            io,
            sync::{Arc, RwLock, atomic::{AtomicU8, Ordering}},
            time::Duration,
        };
        use tokio::net::TcpListener;
        use tower_http::{
            request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
            trace::TraceLayer,
        };
        #contract
        #application
        #routing
        #start_worker
        #start_webhooks
        #startup
        #lifecycle
    })
}

fn contract_source() -> TokenStream {
    quote! {
        #[derive(Clone)]
        pub struct AppState {
            pub database: DatabaseConnection,
            pub extensions: AppExtensions,
            pub auth: AuthState,
            pub mail: MailState,
            pub file: FileState,
            pub realtime: RealtimeState,
            health: ApplicationHealth,
        }
        impl AppState {
            pub async fn context(&self, headers: &HeaderMap) -> Result<RequestContext<'_>, ApiError> {
                let actor = self.auth.actor(&self.database, headers).await?;
                let tenant = tenant::resolve(&self.database, headers, actor.as_ref()).await?;
                Ok(RequestContext::connection_with_services(
                    &self.database, &self.mail, &self.file, &self.realtime, actor, tenant,
                ))
            }
        }

        const HEALTH_STARTING: u8 = 0;
        const HEALTH_READY: u8 = 1;
        const HEALTH_DRAINING: u8 = 2;
        const HEALTH_FAILED: u8 = 3;

        #[derive(Clone)]
        pub struct ApplicationHealth {
            state: Arc<AtomicU8>,
            failure: Arc<RwLock<Option<String>>>,
        }

        impl ApplicationHealth {
            fn with_state(state: u8) -> Self {
                Self {
                    state: Arc::new(AtomicU8::new(state)),
                    failure: Arc::new(RwLock::new(None)),
                }
            }
            fn starting() -> Self { Self::with_state(HEALTH_STARTING) }
            fn ready() -> Self { Self::with_state(HEALTH_READY) }
            fn mark_ready(&self) { self.state.store(HEALTH_READY, Ordering::Release); }
            fn mark_draining(&self) { self.state.store(HEALTH_DRAINING, Ordering::Release); }
            pub fn mark_failed(&self, message: impl Into<String>) {
                if self.state.load(Ordering::Acquire) == HEALTH_DRAINING { return; }
                let message = message.into();
                if let Ok(mut failure) = self.failure.write() { *failure = Some(message.clone()); }
                self.state.store(HEALTH_FAILED, Ordering::Release);
                tracing::error!(%message, "application health failed");
            }
            fn is_ready(&self) -> bool {
                let no_failure = self.failure.read().is_ok_and(|failure| failure.is_none());
                self.state.load(Ordering::Acquire) == HEALTH_READY && no_failure
            }
        }
    }
}

fn application_source() -> TokenStream {
    quote! {
        pub struct Application {
            router: Router,
            runtime: ModuleRuntime,
            health: ApplicationHealth,
        }
        impl Application {
            pub async fn from_env(
                database: DatabaseConnection, extensions: AppExtensions,
            ) -> Result<Self, StartupError> {
                let health = ApplicationHealth::starting();
                let started = start_application_modules(
                    database, extensions, health.clone(),
                ).await?;
                health.mark_ready();
                let router = router_with_services_and_health(
                    started.database, started.extensions, started.auth, started.mail, started.file,
                    health.clone(),
                );
                Ok(Self { router, runtime: started.runtime, health })
            }

            pub fn with_services(
                database: DatabaseConnection, extensions: AppExtensions, auth: AuthState,
                mail: MailState, file: FileState,
            ) -> Self {
                let mut runtime = ModuleRuntime::default();
                let health = ApplicationHealth::starting();
                if let Some(worker) = start_job_worker(
                    &database, &extensions, &mail, health.clone(),
                ) {
                    runtime.push_service("appstruct/jobs", worker);
                }
                if let Some(worker) = start_webhook_worker(&database) {
                    runtime.push_service("appstruct/webhooks", worker);
                }
                health.mark_ready();
                let router = router_with_services_and_health(
                    database, extensions, auth, mail, file, health.clone(),
                );
                Self { router, runtime, health }
            }

            pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
                let Self { router, mut runtime, health } = self;
                let (shutdown, receiver) = tokio::sync::oneshot::channel::<()>();
                let server = axum::serve(listener, router).with_graceful_shutdown(async move {
                    let _ = receiver.await;
                }).into_future();
                tokio::pin!(server);
                let result = tokio::select! {
                    result = &mut server => result,
                    signal = shutdown_signal() => {
                        health.mark_draining();
                        if signal.is_ok() {
                            tracing::info!("shutdown signal received");
                        }
                        let _ = shutdown.send(());
                        let server_result = match tokio::time::timeout(
                            Duration::from_secs(30), server.as_mut(),
                        ).await {
                            Ok(result) => result,
                            Err(_) => Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "HTTP server did not drain within 30 seconds",
                            )),
                        };
                        match signal {
                            Ok(()) => server_result,
                            Err(error) => {
                                let _ = server_result;
                                Err(error)
                            }
                        }
                    }
                };
                health.mark_draining();
                let shutdown = stop_modules(&mut runtime).await;
                match (result, shutdown) {
                    (Err(error), _) => Err(error),
                    (Ok(()), Err(error)) => Err(error),
                    (Ok(()), Ok(())) => Ok(()),
                }
            }
        }
        pub fn router(
            database: DatabaseConnection, extensions: AppExtensions,
        ) -> Result<Router, StartupError> {
            let auth = AuthState::from_env()
                .map_err(|error| StartupError::configuration("auth", error))?;
            router_with_auth(database, extensions, auth)
        }

        pub fn router_with_auth(
            database: DatabaseConnection, extensions: AppExtensions, auth: AuthState,
        ) -> Result<Router, StartupError> {
            let mail = MailState::from_env(database.clone())
                .map_err(|error| StartupError::configuration("mail", error))?;
            let file = FileState::from_env(database.clone())
                .map_err(|error| StartupError::configuration("file", error))?;
            Ok(router_with_services(database, extensions, auth, mail, file))
        }
    }
}

fn router_source(routes: &[TokenStream]) -> TokenStream {
    quote! {
        pub fn router_with_services(
            database: DatabaseConnection, extensions: AppExtensions, auth: AuthState,
            mail: MailState, file: FileState,
        ) -> Router {
            router_with_services_and_health(
                database, extensions, auth, mail, file, ApplicationHealth::ready(),
            )
        }

        fn router_with_services_and_health(
            database: DatabaseConnection, extensions: AppExtensions, auth: AuthState,
            mail: MailState, file: FileState, health: ApplicationHealth,
        ) -> Router {
            let cors = auth.cors_layer();
            Router::new()
                #(#routes)*
                .merge(operations::router()).merge(audit::router())
                .merge(auth::router()).merge(tenant::router())
                .merge(realtime::router())
                .route("/health/live", get(liveness)).route("/health/ready", get(readiness))
                .route("/metrics", get(metrics))
                .route("/openapi.json", get(openapi)).layer(cors)
                .layer(PropagateRequestIdLayer::x_request_id()).layer(TraceLayer::new_for_http())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .with_state(AppState {
                    realtime: RealtimeState::new(database.clone()),
                    database, extensions, auth, mail, file, health,
                })
        }
        async fn liveness() -> StatusCode { StatusCode::NO_CONTENT }
        async fn readiness(State(state): State<AppState>) -> StatusCode {
            if state.health.is_ready() && state.database.ping().await.is_ok() {
                StatusCode::NO_CONTENT
            }
            else { StatusCode::SERVICE_UNAVAILABLE }
        }
        async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
            let ready = u8::from(state.health.is_ready());
            let body = format!(
                "# HELP appstruct_health_ready Whether the application is ready to serve traffic.\n# TYPE appstruct_health_ready gauge\nappstruct_health_ready {ready}\n"
            );
            ([(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
        }
        async fn openapi() -> impl IntoResponse {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], openapi::OPENAPI_JSON)
        }
    }
}

fn lifecycle_source() -> TokenStream {
    quote! {
        async fn stop_modules(runtime: &mut ModuleRuntime) -> io::Result<()> {
            let report = runtime.shutdown_reverse().await;
            for module in &report.attempted {
                tracing::info!(module, "module stopped");
            }
            for failure in &report.failures {
                tracing::error!(
                    service = %failure.service,
                    kind = ?failure.kind,
                    detail = %failure.message,
                    "module shutdown failed",
                );
            }
            if report.is_success() { Ok(()) } else { Err(io::Error::other(report.to_string())) }
        }

        async fn shutdown_signal() -> io::Result<()> {
            #[cfg(unix)]
            {
                let mut terminate = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::terminate(),
                )?;
                tokio::select! {
                    result = tokio::signal::ctrl_c() => result,
                    _ = terminate.recv() => Ok(()),
                }
            }
            #[cfg(not(unix))]
            { tokio::signal::ctrl_c().await }
        }
    }
}

fn start_worker(ir: &AppIr) -> TokenStream {
    if !ir.jobs.enabled {
        return quote! {
            fn start_job_worker(
                _database: &DatabaseConnection, _extensions: &AppExtensions, _mail: &MailState,
                _health: ApplicationHealth,
            ) -> Option<JobWorkerHandle> { None }
        };
    }
    if ir.mail.enabled {
        quote! {
            fn start_job_worker(
                database: &DatabaseConnection, extensions: &AppExtensions, mail: &MailState,
                health: ApplicationHealth,
            ) -> Option<JobWorkerHandle> {
                let worker = if let Some(handler) = extensions.job_handler() {
                    JobWorker::new(database.clone(), handler)
                } else {
                    JobWorker::for_kind(
                        database.clone(),
                        std::sync::Arc::new(MailJobHandler::new(mail.clone())),
                        "mail.send",
                    )
                };
                Some(worker.spawn_with_health(health))
            }
        }
    } else {
        quote! {
            fn start_job_worker(
                database: &DatabaseConnection, extensions: &AppExtensions, _mail: &MailState,
                health: ApplicationHealth,
            ) -> Option<JobWorkerHandle> {
                let Some(handler) = extensions.job_handler() else {
                    tracing::warn!("jobs enabled without a registered job handler; worker not started");
                    return None;
                };
                Some(JobWorker::new(database.clone(), handler).spawn_with_health(health))
            }
        }
    }
}

fn start_webhook_worker(ir: &AppIr) -> TokenStream {
    if ir.webhooks.enabled {
        quote! {
            fn start_webhook_worker(database: &DatabaseConnection) -> Option<WebhookWorkerHandle> {
                Some(WebhookWorker::new(database.clone()).spawn())
            }
        }
    } else {
        quote! {
            fn start_webhook_worker(_database: &DatabaseConnection) -> Option<WebhookWorkerHandle> {
                None
            }
        }
    }
}
