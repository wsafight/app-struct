use crate::environment::ProjectEnvironment;
use appstruct_ir::{DatabaseDevMode, DatabaseMigrationPolicy};
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

mod build_cache;
mod process;
mod watch;

use process::{DevProcesses, ManagedDatabase};
use watch::{ProjectChanges, ProjectWatcher};

const MANAGED_DATABASE_URL: &str =
    "postgresql://appstruct:appstruct-dev@127.0.0.1:5432/appstruct?sslmode=disable";

pub(crate) fn run(project: &Path, api_port: u16, web_port: u16) -> ExitCode {
    if api_port == 0 || web_port == 0 || api_port == web_port {
        return crate::report::fail(
            "AS6005",
            crate::report::ErrorCategory::Development,
            "API and web ports must be non-zero and different",
            crate::report::ExitClass::Usage,
        );
    }
    let stopping = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&stopping);
    if let Err(error) = ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst)) {
        return crate::report::fail(
            "AS6005",
            crate::report::ErrorCategory::Development,
            format!("cannot install Ctrl-C handler: {error}"),
            crate::report::ExitClass::Environment,
        );
    }
    match DevSession::start(project, api_port, web_port, stopping)
        .and_then(|mut session| session.run())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::Interrupted => ExitCode::SUCCESS,
        Err(error) => crate::report::fail(
            "AS6005",
            crate::report::ErrorCategory::Development,
            format!("development server failed: {error}"),
            crate::report::ExitClass::Environment,
        ),
    }
}

struct DevSession<'project> {
    project: &'project Path,
    environment: ProjectEnvironment,
    database_url: String,
    database_mode: DatabaseDevMode,
    migration_policy: DatabaseMigrationPolicy,
    database: ManagedDatabase,
    processes: DevProcesses,
    watcher: ProjectWatcher,
    api_port: u16,
    web_port: u16,
    stopping: Arc<AtomicBool>,
}

impl<'project> DevSession<'project> {
    fn start(
        project: &'project Path,
        api_port: u16,
        web_port: u16,
        stopping: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        check_stopping(&stopping)?;
        let ir = compile(project)?;
        let environment = ProjectEnvironment::load(project)?;
        let database_mode = ir.database.dev_mode;
        let migration_policy = ir.database.dev_migration;
        let (database, database_url) = match database_mode {
            DatabaseDevMode::Managed => {
                let database = ManagedDatabase::start(project, &environment)?;
                let url = environment
                    .get("DATABASE_URL")
                    .unwrap_or_else(|| MANAGED_DATABASE_URL.to_owned());
                wait_for_database(&url, &stopping)?;
                (database, url)
            }
            DatabaseDevMode::External => {
                let url = environment.get("DATABASE_URL").ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "DATABASE_URL is required for external database mode",
                    )
                })?;
                (ManagedDatabase::external(), url)
            }
        };
        check_stopping(&stopping)?;
        prepare(
            project,
            &database_url,
            &environment,
            migration_policy,
            true,
            &stopping,
        )?;
        let processes =
            DevProcesses::spawn(project, &environment, &database_url, api_port, web_port)?;
        check_stopping(&stopping)?;
        let watcher = ProjectWatcher::start(project)?;
        println!("AppStruct development environment is ready:");
        println!("- API: http://127.0.0.1:{api_port}");
        println!("- Web: http://127.0.0.1:{web_port}");
        Ok(Self {
            project,
            environment,
            database_url,
            database_mode,
            migration_policy,
            database,
            processes,
            watcher,
            api_port,
            web_port,
            stopping,
        })
    }

    fn run(&mut self) -> io::Result<()> {
        while !self.stopping.load(Ordering::SeqCst) {
            if let Some(failure) = self.processes.failure()? {
                return Err(io::Error::other(failure));
            }
            if let Some(changes) = self.watcher.changes(Duration::from_millis(250))? {
                self.watcher.refresh()?;
                self.reload(changes);
            }
        }
        self.processes.stop();
        self.database.stop()
    }

    fn reload(&mut self, changes: ProjectChanges) {
        if changes.specification {
            self.reload_specification();
        } else if changes.backend {
            self.reload_backend();
        } else if changes.web {
            println!("[appstruct] web extension changed; Vite will apply the update");
        }
    }

    fn reload_specification(&mut self) {
        println!("[appstruct] App Spec inputs changed; regenerating");
        let result = match self.refresh_environment() {
            Ok(()) => prepare(
                self.project,
                &self.database_url,
                &self.environment,
                self.migration_policy,
                false,
                &self.stopping,
            )
            .and_then(|()| {
                self.processes.restart(
                    self.project,
                    &self.environment,
                    &self.database_url,
                    self.api_port,
                    self.web_port,
                )
            }),
            Err(error) if error.kind() == io::ErrorKind::Unsupported => {
                crate::report::warning(
                    "AS6006",
                    crate::report::ErrorCategory::Development,
                    &error.to_string(),
                );
                return;
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => println!("[appstruct] generated services restarted"),
            Err(error) => {
                eprintln!("[appstruct] rebuild failed; services were not restarted: {error}");
            }
        }
    }

    fn reload_backend(&mut self) {
        println!("[appstruct] backend extension changed; rebuilding API");
        let result = (|| {
            if crate::generation::run(self.project, false) != ExitCode::SUCCESS {
                return Err(io::Error::other("generation failed"));
            }
            check_stopping(&self.stopping)?;
            build_backend(self.project, &self.environment)?;
            check_stopping(&self.stopping)?;
            self.processes.restart_api(
                self.project,
                &self.environment,
                &self.database_url,
                self.api_port,
                self.web_port,
            )
        })();
        match result {
            Ok(()) => println!("[appstruct] API restarted"),
            Err(error) => {
                eprintln!("[appstruct] API rebuild failed; the process was not restarted: {error}");
            }
        }
    }

    fn refresh_environment(&mut self) -> io::Result<()> {
        let ir = compile(self.project)?;
        if ir.database.dev_mode != self.database_mode {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "database development mode changed; restart `appstruct dev` to reconfigure it",
            ));
        }
        if ir.database.dev_migration != self.migration_policy {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "database migration policy changed; restart `appstruct dev` to apply it",
            ));
        }
        let environment = ProjectEnvironment::load(self.project)?;
        let database_url = match self.database_mode {
            DatabaseDevMode::Managed => environment
                .get("DATABASE_URL")
                .unwrap_or_else(|| MANAGED_DATABASE_URL.to_owned()),
            DatabaseDevMode::External => environment.get("DATABASE_URL").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "DATABASE_URL is required for external database mode",
                )
            })?,
        };
        if self.database_mode == DatabaseDevMode::Managed {
            wait_for_database(&database_url, &self.stopping)?;
        }
        self.database.update_environment(environment.clone());
        self.environment = environment;
        self.database_url = database_url;
        Ok(())
    }
}

impl Drop for DevSession<'_> {
    fn drop(&mut self) {
        self.processes.stop();
        let _ = self.database.stop();
    }
}

fn compile(project: &Path) -> io::Result<appstruct_ir::AppIr> {
    appstruct_compiler::compile_project(project).map_err(|diagnostics| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })
}

fn prepare(
    project: &Path,
    database_url: &str,
    environment: &ProjectEnvironment,
    migration_policy: DatabaseMigrationPolicy,
    announce_unmanaged: bool,
    stopping: &AtomicBool,
) -> io::Result<()> {
    check_stopping(stopping)?;
    crate::migration::prepare_development(
        project,
        database_url,
        migration_policy,
        announce_unmanaged,
    )?;
    check_stopping(stopping)?;
    if crate::generation::run(project, false) != ExitCode::SUCCESS {
        return Err(io::Error::other("generation failed"));
    }
    check_stopping(stopping)?;
    build_backend(project, environment)?;
    check_stopping(stopping)?;
    install_web(project, environment)
}

fn build_backend(project: &Path, environment: &ProjectEnvironment) -> io::Result<()> {
    if build_cache::backend_current(project, environment)? {
        println!("[appstruct] backend inputs unchanged; reusing debug build");
        return Ok(());
    }
    let backend = crate::build::backend_manifest(project)?;
    let target = project.join(".appstruct/cache/backend-target");
    let lock = backend
        .parent()
        .expect("backend manifest has a parent")
        .join("Cargo.lock");
    let mut command = Command::new("cargo");
    command.current_dir(project).env("CARGO_TARGET_DIR", target);
    if lock.is_file() {
        command.arg("build").arg("--locked");
    } else {
        command.arg("build");
    }
    command.args(["--manifest-path"]).arg(backend);
    environment.apply(&mut command);
    status(command, "build generated backend")?;
    build_cache::record_backend(project, environment)
}

fn install_web(project: &Path, environment: &ProjectEnvironment) -> io::Result<()> {
    if build_cache::web_dependencies_current(project, environment)? {
        println!("[appstruct] web dependencies unchanged; reusing installation");
        return Ok(());
    }
    let mut command = Command::new("pnpm");
    command
        .current_dir(project.join("generated/web"))
        .args(["install", "--frozen-lockfile"]);
    environment.apply(&mut command);
    status(command, "install generated web dependencies")?;
    build_cache::record_web_dependencies(project, environment)
}

fn wait_for_database(database_url: &str, stopping: &AtomicBool) -> io::Result<()> {
    for _ in 0..60 {
        check_stopping(stopping)?;
        match appstruct_migrate::connect_database(database_url) {
            Ok(_) => return Ok(()),
            Err(appstruct_migrate::MigrationError::Database(_)) => {}
            Err(error) => return Err(io::Error::other(error)),
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "managed PostgreSQL did not become ready within 30 seconds",
    ))
}

fn check_stopping(stopping: &AtomicBool) -> io::Result<()> {
    if stopping.load(Ordering::SeqCst) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "development server startup interrupted",
        ))
    } else {
        Ok(())
    }
}

fn status(mut command: Command, context: &str) -> io::Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{context}: process exited with {status}"
        )))
    }
}

#[cfg(test)]
mod tests;
