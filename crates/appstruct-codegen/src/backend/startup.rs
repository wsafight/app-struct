use crate::CodegenError;
use appstruct_ir::{AppIr, ModuleOrigin, ResolvedModule};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeMap;

pub(super) fn source(ir: &AppIr) -> Result<TokenStream, CodegenError> {
    let auth = disabled_default(ir.auth.enabled, &quote! { AuthState::default() });
    let mail = disabled_default(ir.mail.enabled, &quote! { MailState::default() });
    let file = disabled_default(ir.file.enabled, &quote! { FileState::default() });
    let module_plan = module_plan(ir)?;
    let observer = observer_source();
    let connect = connect_database_source();
    Ok(quote! {
        struct StartupContext {
            database: DatabaseConnection,
            extensions: AppExtensions,
            health: ApplicationHealth,
            auth: Option<AuthState>,
            mail: Option<MailState>,
            file: Option<FileState>,
        }

        struct ApplicationParts {
            database: DatabaseConnection,
            extensions: AppExtensions,
            auth: AuthState,
            mail: MailState,
            file: FileState,
        }

        struct StartedApplication {
            database: DatabaseConnection,
            extensions: AppExtensions,
            auth: AuthState,
            mail: MailState,
            file: FileState,
            runtime: ModuleRuntime,
        }

        impl StartupContext {
            fn new(
                database: DatabaseConnection,
                extensions: AppExtensions,
                health: ApplicationHealth,
            ) -> Self {
                Self {
                    database,
                    extensions,
                    health,
                    auth: #auth,
                    mail: #mail,
                    file: #file,
                }
            }

            fn finish(self) -> Result<ApplicationParts, StartupError> {
                let _health = self.health;
                Ok(ApplicationParts {
                    database: self.database,
                    extensions: self.extensions,
                    auth: self.auth.ok_or_else(|| missing_module_state("appstruct/auth"))?,
                    mail: self.mail.ok_or_else(|| missing_module_state("appstruct/mail"))?,
                    file: self.file.ok_or_else(|| missing_module_state("appstruct/file"))?,
                })
            }
        }

        fn missing_module_state(module: &'static str) -> StartupError {
            StartupError::configuration(module, "runtime plan did not initialize module state")
        }

        #observer

        async fn start_application_modules(
            database: DatabaseConnection,
            extensions: AppExtensions,
            health: ApplicationHealth,
        ) -> Result<StartedApplication, StartupError> {
            let mut context = StartupContext::new(database, extensions, health);
            let mut runtime = startup_plan().start(&mut context).await?;
            let parts = match context.finish() {
                Ok(parts) => parts,
                Err(error) => {
                    let service = error.service().to_owned();
                    let rollback = runtime.rollback_reverse().await;
                    return Err(error.with_runtime_context(vec![service], rollback));
                }
            };
            Ok(StartedApplication {
                database: parts.database,
                extensions: parts.extensions,
                auth: parts.auth,
                mail: parts.mail,
                file: parts.file,
                runtime,
            })
        }

        #module_plan
        #connect
    })
}

fn connect_database_source() -> TokenStream {
    quote! {
        pub async fn connect_database(
            database_url: impl Into<String>,
        ) -> Result<DatabaseConnection, sea_orm::DbErr> {
            let mut options = sea_orm::ConnectOptions::new(database_url.into());
            let max_connections = env_positive_u32("APPSTRUCT_DB_MAX_CONNECTIONS", 20);
            let min_connections =
                env_positive_u32("APPSTRUCT_DB_MIN_CONNECTIONS", 1).min(max_connections);
            options.max_connections(max_connections);
            options.min_connections(min_connections);
            options.connect_timeout(Duration::from_secs(env_positive_u64(
                "APPSTRUCT_DB_CONNECT_TIMEOUT_SECS",
                8,
            )));
            options.acquire_timeout(Duration::from_secs(env_positive_u64(
                "APPSTRUCT_DB_ACQUIRE_TIMEOUT_SECS",
                8,
            )));
            options.idle_timeout(Duration::from_secs(env_positive_u64(
                "APPSTRUCT_DB_IDLE_TIMEOUT_SECS",
                300,
            )));
            options.max_lifetime(Duration::from_secs(env_positive_u64(
                "APPSTRUCT_DB_MAX_LIFETIME_SECS",
                1800,
            )));
            sea_orm::Database::connect(options).await
        }

        fn env_positive_u32(name: &str, default: u32) -> u32 {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0)
                .unwrap_or(default)
        }

        fn env_positive_u64(name: &str, default: u64) -> u64 {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0)
                .unwrap_or(default)
        }
    }
}

fn observer_source() -> TokenStream {
    quote! {
        struct TracingModuleObserver;

        impl ModuleObserver for TracingModuleObserver {
            fn observe(&self, event: &ModuleEvent) {
                match event.phase {
                    ModulePhase::Failed | ModulePhase::StopFailed | ModulePhase::RollbackFailed => tracing::error!(
                        module = %event.module,
                        detail = event.detail.as_deref().unwrap_or_default(),
                        "module lifecycle failure",
                    ),
                    ModulePhase::RollingBack | ModulePhase::RolledBack => tracing::warn!(
                        module = %event.module,
                        phase = ?event.phase,
                        "module lifecycle rollback",
                    ),
                    _ => tracing::info!(
                        module = %event.module,
                        phase = ?event.phase,
                        "module lifecycle transition",
                    ),
                }
            }
        }
    }
}

fn disabled_default(enabled: bool, value: &TokenStream) -> TokenStream {
    if enabled {
        quote! { None }
    } else {
        quote! { Some(#value) }
    }
}

fn module_plan(ir: &AppIr) -> Result<TokenStream, CodegenError> {
    if ir.modules.is_empty() {
        return Ok(quote! {
            fn startup_plan() -> ModulePlan<StartupContext> {
                ModulePlan::new().with_observer(TracingModuleObserver)
            }
        });
    }

    let providers = capability_providers(&ir.modules);
    let variants = ir
        .modules
        .iter()
        .map(module_variant)
        .collect::<Result<Vec<_>, _>>()?;
    let descriptors = ir
        .modules
        .iter()
        .zip(&variants)
        .map(|(module, variant)| descriptor_arm(module, variant, &providers))
        .collect::<Result<Vec<_>, _>>()?;
    let starters = ir
        .modules
        .iter()
        .zip(&variants)
        .map(|(module, variant)| starter_arm(module, variant))
        .collect::<Result<Vec<_>, _>>()?;
    let pushes = variants
        .iter()
        .map(|variant| quote! { plan.push(GeneratedModule::#variant); });

    Ok(quote! {
        enum GeneratedModule { #(#variants,)* }

        #[async_trait::async_trait]
        impl ModuleStarter<StartupContext> for GeneratedModule {
            fn descriptor(&self) -> ModuleDescriptor {
                match self { #(#descriptors,)* }
            }

            async fn start(
                self: Box<Self>,
                context: &mut StartupContext,
            ) -> Result<Option<Box<dyn ServiceHandle>>, StartupError> {
                match *self { #(#starters,)* }
            }
        }

        fn startup_plan() -> ModulePlan<StartupContext> {
            let mut plan = ModulePlan::new().with_observer(TracingModuleObserver);
            #(#pushes)*
            plan
        }
    })
}

fn capability_providers(modules: &[ResolvedModule]) -> BTreeMap<&str, &str> {
    modules
        .iter()
        .flat_map(|module| {
            module
                .provides
                .iter()
                .map(move |capability| (capability.as_str(), module.name.as_str()))
        })
        .collect()
}

fn descriptor_arm(
    module: &ResolvedModule,
    variant: &Ident,
    providers: &BTreeMap<&str, &str>,
) -> Result<TokenStream, CodegenError> {
    let name = &module.name;
    let dependencies = module
        .requires
        .iter()
        .map(|capability| {
            providers.get(capability.as_str()).copied().ok_or_else(|| {
                CodegenError::new(format!(
                    "module `{name}` requires unresolved capability `{capability}`"
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(quote! {
        Self::#variant => ModuleDescriptor::new(#name, &[#(#dependencies),*])
    })
}

fn starter_arm(module: &ResolvedModule, variant: &Ident) -> Result<TokenStream, CodegenError> {
    let name = module.name.as_str();
    if module.origin != ModuleOrigin::Official {
        return Ok(quote! { Self::#variant => Ok(None) });
    }
    let start = match name {
        "appstruct/auth" => quote! {
            let state = AuthState::from_env()
                .map_err(|error| StartupError::configuration(#name, error))?;
            context.auth = Some(state);
            Ok(None)
        },
        "appstruct/mail" => quote! {
            let state = MailState::from_env(context.database.clone())
                .map_err(|error| StartupError::configuration(#name, error))?;
            context.mail = Some(state);
            Ok(None)
        },
        "appstruct/file" => quote! {
            let state = FileState::from_env(context.database.clone())
                .map_err(|error| StartupError::configuration(#name, error))?;
            context.file = Some(state);
            Ok(None)
        },
        "appstruct/jobs" => quote! {
            let mail = context.mail.as_ref()
                .ok_or_else(|| missing_module_state("appstruct/mail"))?;
            let handle = start_job_worker(
                &context.database, &context.extensions, mail, context.health.clone(),
            )
                .map(|handle| Box::new(handle) as Box<dyn ServiceHandle>);
            Ok(handle)
        },
        "appstruct/webhooks" => quote! {
            let handle = start_webhook_worker(&context.database)
                .map(|handle| Box::new(handle) as Box<dyn ServiceHandle>);
            Ok(handle)
        },
        "appstruct/audit" | "appstruct/rbac" | "appstruct/realtime" | "appstruct/tenant" => {
            quote! { Ok(None) }
        }
        _ => {
            return Err(CodegenError::new(format!(
                "module `{name}` has no generated runtime starter"
            )));
        }
    };
    Ok(quote! { Self::#variant => { #start } })
}

fn module_variant(module: &ResolvedModule) -> Result<Ident, CodegenError> {
    if module.origin != ModuleOrigin::Official {
        return Ok(format_ident!("Local{}", module.startup_order));
    }
    match module.name.as_str() {
        "appstruct/auth" => Ok(format_ident!("Auth")),
        "appstruct/audit" => Ok(format_ident!("Audit")),
        "appstruct/file" => Ok(format_ident!("File")),
        "appstruct/jobs" => Ok(format_ident!("Jobs")),
        "appstruct/mail" => Ok(format_ident!("Mail")),
        "appstruct/rbac" => Ok(format_ident!("Rbac")),
        "appstruct/realtime" => Ok(format_ident!("Realtime")),
        "appstruct/tenant" => Ok(format_ident!("Tenant")),
        "appstruct/webhooks" => Ok(format_ident!("Webhooks")),
        name => Err(CodegenError::new(format!(
            "module `{name}` has no generated runtime variant"
        ))),
    }
}
