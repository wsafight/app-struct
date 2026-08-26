use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source(ir: &AppIr, routes: &[TokenStream]) -> TokenStream {
    let contract = contract_source();
    let application = application_source();
    let routing = router_source(routes);
    let start_worker = start_worker(ir);
    let lifecycle = lifecycle_source();
    quote! {
        use axum::{
            Router, extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse,
            routing::get,
        };
        use sea_orm::DatabaseConnection;
        use std::{fmt, future::IntoFuture, io};
        use tokio::net::TcpListener;
        use tower_http::{
            request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
            trace::TraceLayer,
        };
        #contract
        #application
        #routing
        #start_worker
        #lifecycle
    }
}

fn contract_source() -> TokenStream {
    quote! {
        #[derive(Debug)]
        pub struct StartupError { service: &'static str, message: String }
        impl StartupError {
            fn configuration(service: &'static str, error: impl fmt::Display) -> Self {
                Self { service, message: error.to_string() }
            }
            pub fn service(&self) -> &'static str { self.service }
        }
        impl fmt::Display for StartupError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "failed to initialize {}: {}", self.service, self.message)
            }
        }
        impl std::error::Error for StartupError {}
        #[derive(Clone)]
        pub struct AppState {
            pub database: DatabaseConnection,
            pub extensions: AppExtensions,
            pub auth: AuthState,
            pub mail: MailState,
            pub file: FileState,
        }
        impl AppState {
            pub async fn context(&self, headers: &HeaderMap) -> Result<RequestContext<'_>, ApiError> {
                let actor = self.auth.actor(&self.database, headers).await?;
                let tenant = tenant::resolve(&self.database, headers, actor.as_ref()).await?;
                Ok(RequestContext::connection_with_file(
                    &self.database, &self.mail, &self.file, actor, tenant,
                ))
            }
        }
    }
}

fn application_source() -> TokenStream {
    quote! {
        pub struct Application { router: Router, worker: Option<JobWorkerHandle> }
        impl Application {
            pub fn from_env(
                database: DatabaseConnection, extensions: AppExtensions,
            ) -> Result<Self, StartupError> {
                let auth = AuthState::from_env()
                    .map_err(|error| StartupError::configuration("auth", error))?;
                let mail = MailState::from_env(database.clone())
                    .map_err(|error| StartupError::configuration("mail", error))?;
                let file = FileState::from_env(database.clone())
                    .map_err(|error| StartupError::configuration("file", error))?;
                Ok(Self::with_services(database, extensions, auth, mail, file))
            }

            pub fn with_services(
                database: DatabaseConnection, extensions: AppExtensions, auth: AuthState,
                mail: MailState, file: FileState,
            ) -> Self {
                let worker = start_job_worker(&database, &extensions, &mail);
                let router = router_with_services(database, extensions, auth, mail, file);
                Self { router, worker }
            }

            pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
                let Self { router, mut worker } = self;
                let (shutdown, receiver) = tokio::sync::oneshot::channel::<()>();
                let server = axum::serve(listener, router).with_graceful_shutdown(async move {
                    let _ = receiver.await;
                }).into_future();
                tokio::pin!(server);
                let result = tokio::select! {
                    result = &mut server => result,
                    signal = shutdown_signal() => {
                        if signal.is_ok() {
                            tracing::info!("shutdown signal received");
                        }
                        let _ = shutdown.send(());
                        stop_worker(worker.take()).await;
                        let server_result = server.await;
                        match signal {
                            Ok(()) => server_result,
                            Err(error) => {
                                let _ = server_result;
                                Err(error)
                            }
                        }
                    }
                };
                stop_worker(worker).await;
                result
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
            let cors = auth.cors_layer();
            Router::new()
                #(#routes)*
                .merge(operations::router()).merge(audit::router())
                .merge(auth::router()).merge(tenant::router())
                .route("/health/live", get(health)).route("/health/ready", get(readiness))
                .route("/openapi.json", get(openapi)).layer(cors)
                .layer(PropagateRequestIdLayer::x_request_id()).layer(TraceLayer::new_for_http())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .with_state(AppState { database, extensions, auth, mail, file })
        }
        async fn health() -> StatusCode { StatusCode::NO_CONTENT }
        async fn readiness(State(state): State<AppState>) -> StatusCode {
            if state.database.ping().await.is_ok() { StatusCode::NO_CONTENT }
            else { StatusCode::SERVICE_UNAVAILABLE }
        }
        async fn openapi() -> impl IntoResponse {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], openapi::OPENAPI_JSON)
        }
    }
}

fn lifecycle_source() -> TokenStream {
    quote! {
        async fn stop_worker(worker: Option<JobWorkerHandle>) {
            if let Some(worker) = worker {
                worker.shutdown().await;
                tracing::info!("job worker stopped");
            }
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
            ) -> Option<JobWorkerHandle> { None }
        };
    }
    if ir.mail.enabled {
        quote! {
            fn start_job_worker(
                database: &DatabaseConnection, extensions: &AppExtensions, mail: &MailState,
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
                Some(worker.spawn())
            }
        }
    } else {
        quote! {
            fn start_job_worker(
                database: &DatabaseConnection, extensions: &AppExtensions, _mail: &MailState,
            ) -> Option<JobWorkerHandle> {
                let Some(handler) = extensions.job_handler() else {
                    tracing::warn!("jobs enabled without a registered job handler; worker not started");
                    return None;
                };
                Some(JobWorker::new(database.clone(), handler).spawn())
            }
        }
    }
}
