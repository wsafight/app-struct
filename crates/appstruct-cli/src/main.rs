use appstruct_ir::{Diagnostic, Severity};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

mod build;
mod doctor;
mod environment;
mod generation;
mod migration;
mod project_new;

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
    /// Check the local toolchain, database mode, and project configuration.
    Doctor {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Validate the App Spec and build normalized IR in memory.
    Check {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
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
    if let Command::New { name, template } = &cli.command {
        let parent = match cli.project {
            Some(ref path) => path.clone(),
            None => match env::current_dir() {
                Ok(path) => path,
                Err(error) => {
                    eprintln!("error[AS6001]: cannot read current directory: {error}");
                    return ExitCode::from(3);
                }
            },
        };
        return project_new::run(&parent, name, *template);
    }
    let diagnostic_format = match &cli.command {
        Command::Check { format } | Command::Doctor { format } => *format,
        Command::New { .. }
        | Command::Build
        | Command::Generate { .. }
        | Command::Migrate { .. } => OutputFormat::Text,
    };
    let start = match cli.project {
        Some(path) => path,
        None => match env::current_dir() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("error[AS6001]: cannot read current directory: {error}");
                return ExitCode::from(3);
            }
        },
    };
    let project = match appstruct_compiler::discover_project(&start) {
        Ok(project) => project,
        Err(diagnostic) => {
            match diagnostic_format {
                OutputFormat::Text => render_text_diagnostic(&diagnostic),
                OutputFormat::Json => {
                    render_json_report(false, 0, std::slice::from_ref(&diagnostic));
                }
            }
            return ExitCode::from(1);
        }
    };

    match cli.command {
        Command::New { .. } => unreachable!(),
        Command::Build => build::run(&project),
        Command::Doctor { format } => doctor::run(&project, format == OutputFormat::Json),
        Command::Check { format } => match appstruct_compiler::compile_project(&project) {
            Ok(ir) => {
                match format {
                    OutputFormat::Text => println!(
                        "App Spec is valid: {} ({} entities)",
                        ir.app.name,
                        ir.entities.len()
                    ),
                    OutputFormat::Json => render_json_report(true, ir.entities.len(), &[]),
                }
                ExitCode::SUCCESS
            }
            Err(diagnostics) => {
                match format {
                    OutputFormat::Text => {
                        for diagnostic in &diagnostics {
                            render_text_diagnostic(diagnostic);
                        }
                    }
                    OutputFormat::Json => render_json_report(false, 0, &diagnostics),
                }
                ExitCode::from(1)
            }
        },
        Command::Generate { check } => generation::run(&project, check),
        Command::Migrate { command } => migration::run(&project, command),
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
        Err(error) => eprintln!("error[AS5003]: failed to serialize diagnostics: {error}"),
    }
}

pub(crate) fn render_text_diagnostic(diagnostic: &Diagnostic) {
    let span = &diagnostic.primary.span;
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    eprintln!(
        "{}:{}:{}: {severity}[{}]: {}",
        span.file, span.line, span.column, diagnostic.code, diagnostic.message
    );
    if !diagnostic.primary.message.is_empty() {
        eprintln!("  = {}", diagnostic.primary.message);
    }
    for secondary in &diagnostic.secondary {
        eprintln!(
            "  = {}:{}:{}: {}",
            secondary.span.file, secondary.span.line, secondary.span.column, secondary.message
        );
    }
    if let Some(help) = &diagnostic.help {
        eprintln!("  help: {help}");
    }
}
