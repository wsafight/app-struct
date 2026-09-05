use appstruct_migrate::inspect_database_schema;
use clap::Subcommand;
use serde::Serialize;
use similar::TextDiff;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

mod render;

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum DbCommand {
    /// Create an App Spec draft from a live PostgreSQL schema.
    Pull {
        /// PostgreSQL schema to inspect.
        #[arg(long, default_value = "public")]
        schema: String,
        /// Project-relative destination for the generated domain draft.
        #[arg(long, default_value = "spec/imported.yaml")]
        output: PathBuf,
        /// Verify that an existing draft matches the live schema without writing.
        #[arg(long, conflicts_with = "diff")]
        check: bool,
        /// Print a unified diff against an existing draft without writing.
        #[arg(long, conflicts_with = "check")]
        diff: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PullMode {
    Create,
    Check,
    Diff,
}

#[derive(Serialize)]
struct PullResult<'path> {
    command: &'static str,
    action: &'static str,
    schema: String,
    output: &'path Path,
    entity_count: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct PullComparisonResult<'path> {
    command: &'static str,
    action: &'static str,
    schema: String,
    output: &'path Path,
    entity_count: usize,
    warnings: Vec<String>,
    current: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
}

pub(crate) fn run(project: &Path, command: &DbCommand) -> ExitCode {
    match command {
        DbCommand::Pull {
            schema,
            output,
            check,
            diff,
        } => pull(
            project,
            schema,
            output,
            if *check {
                PullMode::Check
            } else if *diff {
                PullMode::Diff
            } else {
                PullMode::Create
            },
        ),
    }
}

fn pull(project: &Path, schema: &str, output: &Path, mode: PullMode) -> ExitCode {
    if let Err(message) = validate_schema_name(schema) {
        return crate::report::fail(
            "AS6301",
            crate::report::ErrorCategory::Configuration,
            message,
            crate::report::ExitClass::Usage,
        );
    }
    let output_path = match resolve_output(project, output, mode) {
        Ok(path) => path,
        Err(error) => {
            return crate::report::fail(
                "AS6302",
                crate::report::ErrorCategory::Project,
                error.to_string(),
                crate::report::ExitClass::Usage,
            );
        }
    };
    let environment = match crate::environment::ProjectEnvironment::load(project) {
        Ok(environment) => environment,
        Err(error) => {
            return crate::report::fail(
                "AS6303",
                crate::report::ErrorCategory::Configuration,
                format!("cannot load project environment: {error}"),
                crate::report::ExitClass::Environment,
            );
        }
    };
    let Some(database_url) = environment.get("DATABASE_URL") else {
        return crate::report::fail(
            "AS6304",
            crate::report::ErrorCategory::Configuration,
            "DATABASE_URL is required for db pull",
            crate::report::ExitClass::Environment,
        );
    };
    let inspection = match inspect_database_schema(&database_url, schema) {
        Ok(inspection) => inspection,
        Err(error) => {
            return crate::report::fail(
                "AS6305",
                crate::report::ErrorCategory::Database,
                error.to_string(),
                crate::report::ExitClass::Database,
            );
        }
    };
    let draft = render::render(&inspection);
    if mode != PullMode::Create {
        return compare_draft(project, &output_path, inspection.name, draft, mode);
    }
    if let Some(parent) = output_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return write_error(&output_path, &error);
    }
    if let Err(error) = crate::transaction::write_new(&output_path, draft.source.as_bytes()) {
        return write_error(&output_path, &error);
    }
    let relative = output_path.strip_prefix(project).unwrap_or(&output_path);
    if crate::report::is_json() {
        crate::report::success(&PullResult {
            command: "db",
            action: "pull",
            schema: inspection.name,
            output: relative,
            entity_count: draft.entity_count,
            warnings: draft.warnings,
        });
    } else {
        println!(
            "Created App Spec draft {} ({} entities)",
            relative.display(),
            draft.entity_count
        );
        for warning in &draft.warnings {
            crate::report::warning("AS6306", crate::report::ErrorCategory::Database, warning);
        }
        println!("Review access rules, then add the draft to appstruct.yaml includes");
    }
    ExitCode::SUCCESS
}

fn compare_draft(
    project: &Path,
    output_path: &Path,
    schema: String,
    draft: render::Draft,
    mode: PullMode,
) -> ExitCode {
    let existing = match fs::read_to_string(output_path) {
        Ok(existing) => existing,
        Err(error) => return read_error(output_path, &error),
    };
    let current = existing == draft.source;
    let relative = output_path.strip_prefix(project).unwrap_or(output_path);
    if mode == PullMode::Check && !current {
        return crate::report::fail(
            "AS6308",
            crate::report::ErrorCategory::Validation,
            format!(
                "App Spec draft `{}` differs from PostgreSQL schema `{schema}`",
                relative.display()
            ),
            crate::report::ExitClass::Validation,
        );
    }
    let diff = (mode == PullMode::Diff && !current)
        .then(|| render_diff(relative, &existing, &draft.source));
    if crate::report::is_json() {
        crate::report::success(&PullComparisonResult {
            command: "db",
            action: if mode == PullMode::Check {
                "check"
            } else {
                "diff"
            },
            schema,
            output: relative,
            entity_count: draft.entity_count,
            warnings: draft.warnings,
            current,
            diff,
        });
    } else {
        for warning in &draft.warnings {
            crate::report::warning("AS6306", crate::report::ErrorCategory::Database, warning);
        }
        if let Some(diff) = diff {
            print!("{diff}");
        } else {
            println!("App Spec draft {} is current", relative.display());
        }
    }
    ExitCode::SUCCESS
}

fn render_diff(path: &Path, existing: &str, expected: &str) -> String {
    TextDiff::from_lines(existing, expected)
        .unified_diff()
        .header(&path.display().to_string(), "live PostgreSQL schema")
        .to_string()
}

fn validate_schema_name(schema: &str) -> Result<(), String> {
    if schema.is_empty() || schema.trim() != schema {
        return Err(
            "database schema must be a non-empty name without surrounding whitespace".into(),
        );
    }
    if schema.len() > 63 || schema.chars().any(char::is_control) {
        return Err(
            "database schema must be at most 63 bytes and contain no control characters".into(),
        );
    }
    Ok(())
}

fn resolve_output(project: &Path, output: &Path, mode: PullMode) -> std::io::Result<PathBuf> {
    if output.is_absolute()
        || output
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(crate::transaction::invalid(
            "db pull output must be a project-relative path without parent traversal",
        ));
    }
    if !matches!(
        output.extension().and_then(|value| value.to_str()),
        Some("yaml" | "yml")
    ) {
        return Err(crate::transaction::invalid(
            "db pull output must use a .yaml or .yml extension",
        ));
    }
    let path = project.join(output);
    let mut current = project.to_path_buf();
    for component in output.parent().into_iter().flat_map(Path::components) {
        let Component::Normal(component) = component else {
            unreachable!("output components were validated");
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(crate::transaction::invalid(format!(
                    "db pull output parent `{}` is a symlink",
                    current.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(crate::transaction::invalid(format!(
                    "db pull output parent `{}` is not a directory",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(crate::transaction::invalid(format!(
                "db pull output `{}` must be a regular file",
                path.display()
            )));
        }
        Ok(_) if mode == PullMode::Create => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("db pull output `{}` already exists", path.display()),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && mode != PullMode::Create => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "db pull {} requires existing output `{}`",
                    if mode == PullMode::Check {
                        "--check"
                    } else {
                        "--diff"
                    },
                    path.display()
                ),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(path)
}

fn write_error(path: &Path, error: &std::io::Error) -> ExitCode {
    crate::report::fail(
        "AS6307",
        crate::report::ErrorCategory::Transaction,
        format!("cannot write App Spec draft `{}`: {error}", path.display()),
        crate::report::ExitClass::Environment,
    )
}

fn read_error(path: &Path, error: &std::io::Error) -> ExitCode {
    crate::report::fail(
        "AS6309",
        crate::report::ErrorCategory::Transaction,
        format!("cannot read App Spec draft `{}`: {error}", path.display()),
        crate::report::ExitClass::Environment,
    )
}

#[cfg(test)]
mod tests;
