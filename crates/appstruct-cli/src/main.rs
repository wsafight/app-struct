use appstruct_ir::Diagnostic;
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

mod auth_admin;
mod build;
mod cache;
mod db;
mod development;
mod doctor;
mod environment;
mod generation;
mod migration;
mod module_registry;
mod preset;
mod project_new;
mod report;
mod schema;
mod transaction;
mod update;

#[derive(Debug, Parser)]
#[command(
    name = "appstruct",
    version,
    about = "Compile AppStruct application specifications"
)]
struct Cli {
    /// Project directory or a path within the project.
    #[arg(long, global = true)]
    project: Option<PathBuf>,

    /// Select human-readable or machine-readable command output.
    #[arg(long, global = true, value_enum, default_value_t = report::OutputFormat::Text)]
    format: report::OutputFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new `AppStruct` project from an official template.
    New {
        name: String,
        #[arg(long, value_enum, default_value_t = project_new::ProjectTemplate::Dashboard)]
        template: project_new::ProjectTemplate,
    },
    /// Build validated backend and web production artifacts.
    Build,
    /// Manage authentication accounts.
    Auth {
        #[command(subcommand)]
        command: auth_admin::AuthCommand,
    },
    /// Check the local toolchain, database mode, and project configuration.
    Doctor {},
    /// Start PostgreSQL coordination, the API, and the Vite development server.
    Dev {
        #[arg(long, default_value_t = 3000)]
        api_port: u16,
        #[arg(long, default_value_t = 5173)]
        web_port: u16,
    },
    /// Inspect an existing PostgreSQL database.
    Db {
        #[command(subcommand)]
        command: db::DbCommand,
    },
    /// Validate the App Spec and build normalized IR in memory.
    Check {
        /// Treat non-fatal App Spec diagnostics as errors.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Generate canonical IR and the minimal Rust backend artifact.
    Generate {
        /// Verify generated files are current without writing them.
        #[arg(long)]
        check: bool,
    },
    /// Plan or accept database schema migrations.
    Migrate {
        #[command(subcommand)]
        command: migration::MigrateCommand,
    },
    /// Install, update, verify, remove, and inspect signed remote modules.
    Module {
        #[command(subcommand)]
        command: module_registry::ModuleCommand,
    },
    /// Inspect the locked official preset and its expanded module defaults.
    Preset {
        #[command(subcommand)]
        command: preset::PresetCommand,
    },
    /// Print the App Spec JSON Schema for editor integration.
    Schema,
    /// Stage, verify, and transactionally commit locked framework updates.
    Update,
}

#[derive(Serialize)]
struct CheckReport<'diagnostic> {
    valid: bool,
    entity_count: usize,
    diagnostics: &'diagnostic [Diagnostic],
}

fn main() -> ExitCode {
    run(Cli::parse())
}

fn run(cli: Cli) -> ExitCode {
    report::set_output_format(cli.format);
    if let Command::New { name, template } = &cli.command {
        let parent = match cli.project {
            Some(ref path) => path.clone(),
            None => match env::current_dir() {
                Ok(path) => path,
                Err(error) => {
                    return report::fail(
                        "AS6001",
                        report::ErrorCategory::Project,
                        format!("cannot read current directory: {error}"),
                        report::ExitClass::Environment,
                    );
                }
            },
        };
        return project_new::run(&parent, name, *template);
    }
    if matches!(&cli.command, Command::Schema) {
        return schema::run();
    }
    let start = match cli.project {
        Some(path) => path,
        None => match env::current_dir() {
            Ok(path) => path,
            Err(error) => {
                return report::fail(
                    "AS6001",
                    report::ErrorCategory::Project,
                    format!("cannot read current directory: {error}"),
                    report::ExitClass::Environment,
                );
            }
        },
    };
    let project = match appstruct_compiler::discover_project(&start) {
        Ok(project) => project,
        Err(diagnostic) => {
            if matches!(&cli.command, Command::Check { .. })
                && cli.format == report::OutputFormat::Json
            {
                render_json_report(false, 0, std::slice::from_ref(&diagnostic));
                return ExitCode::from(1);
            }
            return report::fail_diagnostics(report::ErrorCategory::Project, vec![diagnostic]);
        }
    };

    match cli.command {
        Command::New { .. } | Command::Schema => unreachable!(),
        Command::Auth { command } => auth_admin::run(&project, &command),
        Command::Build => build::run(&project),
        Command::Doctor {} => doctor::run(&project, cli.format == report::OutputFormat::Json),
        Command::Dev { api_port, web_port } => development::run(&project, api_port, web_port),
        Command::Db { command } => db::run(&project, &command),
        Command::Check { deny_warnings } => run_check(&project, cli.format, deny_warnings),
        Command::Generate { check } => generation::run(&project, check),
        Command::Migrate { command } => migration::run(&project, command),
        Command::Module { command } => module_registry::run(&project, &command),
        Command::Preset { command } => preset::run(&project, &command),
        Command::Update => update::run(&project),
    }
}

fn run_check(
    project: &std::path::Path,
    format: report::OutputFormat,
    deny_warnings: bool,
) -> ExitCode {
    match appstruct_compiler::compile_project_report(project) {
        Ok(report) => {
            let denied = deny_warnings && !report.diagnostics.is_empty();
            match format {
                report::OutputFormat::Text => {
                    for diagnostic in &report.diagnostics {
                        report::render_text_diagnostic(diagnostic);
                    }
                    if !denied {
                        println!(
                            "App Spec is valid: {} ({} entities)",
                            report.ir.app.name,
                            report.ir.entities.len()
                        );
                    }
                }
                report::OutputFormat::Json => {
                    render_json_report(!denied, report.ir.entities.len(), &report.diagnostics);
                }
            }
            if denied {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(diagnostics) => {
            match format {
                report::OutputFormat::Text => {
                    for diagnostic in &diagnostics {
                        report::render_text_diagnostic(diagnostic);
                    }
                }
                report::OutputFormat::Json => render_json_report(false, 0, &diagnostics),
            }
            ExitCode::from(1)
        }
    }
}

fn render_json_report(valid: bool, entity_count: usize, diagnostics: &[Diagnostic]) {
    let report = CheckReport {
        valid,
        entity_count,
        diagnostics,
    };
    match serde_json::to_string_pretty(&report) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            let _ = report::fail(
                "AS5003",
                report::ErrorCategory::Validation,
                format!("failed to serialize diagnostics: {error}"),
                report::ExitClass::Environment,
            );
        }
    }
}
