use appstruct_ir::Diagnostic;
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

mod auth_admin;
mod build;
mod cache;
mod capabilities;
mod db;
mod development;
mod doctor;
mod environment;
mod fingerprint;
mod generation;
mod migration;
mod module_registry;
mod preset;
mod project_diagnostics;
mod project_new;
mod project_status;
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
    /// Show the Auth and Billing provider support matrix.
    Capabilities,
    /// Create a new `AppStruct` project from an official template.
    New {
        name: String,
        #[arg(long, value_enum, default_value_t = project_new::ProjectTemplate::Dashboard)]
        template: project_new::ProjectTemplate,
    },
    /// Interactively create a new `AppStruct` project.
    Init(project_new::InitArgs),
    /// Build validated backend and web production artifacts.
    Build,
    /// Manage authentication accounts.
    Auth {
        #[command(subcommand)]
        command: auth_admin::AuthCommand,
    },
    /// Check the local toolchain, database mode, and project configuration.
    Doctor {},
    /// Start PostgreSQL coordination, the API, and the Rsbuild development server.
    Dev {
        #[arg(long)]
        api_port: Option<u16>,
        #[arg(long)]
        web_port: Option<u16>,
        /// Do not open the generated Web application in the default browser.
        #[arg(long)]
        no_open: bool,
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
        /// Report compiler, planner, formatter, and output timings.
        #[arg(long)]
        timings: bool,
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
    /// Inspect the local project control-plane status.
    Project {
        #[command(subcommand)]
        command: project_status::ProjectCommand,
    },
    /// Show the local project control-plane status.
    Status,
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
        let parent = match project_start(cli.project.as_ref()) {
            Ok(path) => path,
            Err(exit) => return exit,
        };
        return project_new::run(&parent, name, *template);
    }
    if let Command::Init(args) = &cli.command {
        let parent = match project_start(cli.project.as_ref()) {
            Ok(path) => path,
            Err(exit) => return exit,
        };
        return project_new::init(&parent, args);
    }
    if matches!(&cli.command, Command::Schema | Command::Capabilities) {
        if matches!(&cli.command, Command::Capabilities) {
            return capabilities::run();
        }
        return schema::run();
    }
    let start = match project_start(cli.project.as_ref()) {
        Ok(path) => path,
        Err(exit) => return exit,
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
        Command::New { .. } | Command::Init(_) | Command::Schema | Command::Capabilities => {
            unreachable!()
        }
        Command::Auth { command } => auth_admin::run(&project, &command),
        Command::Build => build::run(&project),
        Command::Doctor {} => doctor::run(&project, cli.format == report::OutputFormat::Json),
        Command::Dev {
            api_port,
            web_port,
            no_open,
        } => development::run(&project, api_port, web_port, !no_open),
        Command::Db { command } => db::run(&project, &command),
        Command::Check { deny_warnings } => run_check(&project, cli.format, deny_warnings),
        Command::Generate { check, timings } => {
            generation::run_with_timings(&project, check, timings)
        }
        Command::Migrate { command } => migration::run(&project, command),
        Command::Module { command } => module_registry::run(&project, &command),
        Command::Preset { command } => preset::run(&project, &command),
        Command::Update => update::run(&project),
        Command::Project { command } => {
            project_diagnostics::run(&project, &command, cli.format == report::OutputFormat::Json)
        }
        Command::Status => project_status::run(&project, cli.format == report::OutputFormat::Json),
    }
}

fn project_start(project: Option<&PathBuf>) -> Result<PathBuf, ExitCode> {
    if let Some(path) = project {
        return Ok(path.clone());
    }
    env::current_dir().map_err(|error| {
        report::fail(
            "AS6001",
            report::ErrorCategory::Project,
            format!("cannot read current directory: {error}"),
            report::ExitClass::Environment,
        )
    })
}

fn run_check(
    project: &std::path::Path,
    format: report::OutputFormat,
    deny_warnings: bool,
) -> ExitCode {
    match appstruct_compiler::compile_project_report(project) {
        Ok(report) => {
            let mut diagnostics = report.diagnostics;
            match appstruct_codegen::check_user_symbols(project, &report.ir) {
                Ok(missing) => diagnostics.extend(missing),
                Err(error) => {
                    return report::fail(
                        "AS5008",
                        report::ErrorCategory::Generation,
                        format!("cannot inspect app/web sources: {error}"),
                        report::ExitClass::Environment,
                    );
                }
            }
            // Errors always fail the check; `--deny-warnings` additionally promotes warnings.
            let has_error = diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == appstruct_ir::Severity::Error);
            let denied = has_error || (deny_warnings && !diagnostics.is_empty());
            match format {
                report::OutputFormat::Text => {
                    for diagnostic in &diagnostics {
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
                    render_json_report(!denied, report.ir.entities.len(), &diagnostics);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
    }

    #[test]
    fn check_commands_succeed_for_the_m0_fixture() {
        assert_eq!(
            run(Cli {
                project: Some(fixture()),
                format: report::OutputFormat::Json,
                command: Command::Check {
                    deny_warnings: false,
                },
            }),
            ExitCode::SUCCESS
        );
        assert_eq!(
            run(Cli {
                project: Some(fixture()),
                format: report::OutputFormat::Text,
                command: Command::Check {
                    deny_warnings: false,
                },
            }),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn missing_projects_fail_for_json_and_text_check() {
        let missing = PathBuf::from("/missing-appstruct-project");
        assert_ne!(
            run(Cli {
                project: Some(missing.clone()),
                format: report::OutputFormat::Json,
                command: Command::Check {
                    deny_warnings: false,
                },
            }),
            ExitCode::SUCCESS
        );
        assert_ne!(
            run(Cli {
                project: Some(missing),
                format: report::OutputFormat::Text,
                command: Command::Doctor {},
            }),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn migrate_plan_and_preset_show_cover_command_dispatch() {
        assert_eq!(
            run(Cli {
                project: Some(fixture()),
                format: report::OutputFormat::Text,
                command: Command::Migrate {
                    command: migration::MigrateCommand::Plan,
                },
            }),
            ExitCode::SUCCESS
        );
        assert_ne!(
            run(Cli {
                project: Some(fixture()),
                format: report::OutputFormat::Json,
                command: Command::Preset {
                    command: preset::PresetCommand::Show { expanded: false },
                },
            }),
            ExitCode::SUCCESS
        );
        assert_eq!(
            run(Cli {
                project: Some(fixture()),
                format: report::OutputFormat::Text,
                command: Command::Module {
                    command: module_registry::ModuleCommand::List,
                },
            }),
            ExitCode::SUCCESS
        );
    }
}
