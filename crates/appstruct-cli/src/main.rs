use appstruct_codegen::Artifact;
use appstruct_ir::{Diagnostic, Severity};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

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
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
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
    let diagnostic_format = match &cli.command {
        Command::Check { format } => *format,
        Command::Generate { .. } => OutputFormat::Text,
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
        Command::Generate { check } => generate(&project, check),
    }
}

fn generate(project: &Path, check: bool) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    let artifacts = match appstruct_codegen::plan(&ir) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            eprintln!("error[AS5001]: {error}");
            return ExitCode::from(1);
        }
    };

    let generated_root = project.join("generated");
    if check {
        let stale = stale_artifacts(&generated_root, &artifacts);
        if stale.is_empty() {
            println!(
                "Generated artifacts are current ({} files)",
                artifacts.len()
            );
            return ExitCode::SUCCESS;
        }
        for path in stale {
            eprintln!("stale generated artifact: {}", path.display());
        }
        return ExitCode::from(1);
    }

    match write_artifacts(&generated_root, &artifacts) {
        Ok(changed) => {
            println!(
                "Generated {} artifacts for {} ({changed} changed)",
                artifacts.len(),
                ir.app.name
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[AS5002]: failed to write generated artifacts: {error}");
            ExitCode::from(3)
        }
    }
}

fn stale_artifacts(root: &Path, artifacts: &[Artifact]) -> Vec<PathBuf> {
    artifacts
        .iter()
        .filter_map(|artifact| {
            let path = root.join(&artifact.relative_path);
            match fs::read(&path) {
                Ok(content) if content == artifact.content => None,
                _ => Some(path),
            }
        })
        .collect()
}

fn write_artifacts(root: &Path, artifacts: &[Artifact]) -> io::Result<usize> {
    let mut changed = 0;
    for artifact in artifacts {
        validate_relative_path(&artifact.relative_path)?;
        let path = root.join(&artifact.relative_path);
        if fs::read(&path).is_ok_and(|content| content == artifact.content) {
            continue;
        }
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact has no parent directory",
            )
        })?;
        fs::create_dir_all(parent)?;
        fs::write(path, &artifact.content)?;
        changed += 1;
    }
    Ok(changed)
}

fn validate_relative_path(path: &Path) -> io::Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsafe artifact path `{}`", path.display()),
        ));
    }
    Ok(())
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

fn render_text_diagnostic(diagnostic: &Diagnostic) {
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
