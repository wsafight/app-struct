use appstruct_migrate::inspect_database_schema;
use clap::Subcommand;
use serde::Serialize;
use similar::TextDiff;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod access;
mod output;
mod preview;
mod render;
mod review;

use access::{resolve_access, valid_access_name};
#[cfg(test)]
use output::resolve as resolve_output;

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
        /// Review and select tables in a terminal before writing the draft.
        #[arg(long, conflicts_with_all = ["check", "diff"])]
        review: bool,
        /// Access policy to add to imported entities.
        #[arg(long, value_enum, default_value = "none")]
        access: PullAccess,
        /// RBAC role used when --access role is selected.
        #[arg(long, conflicts_with = "owner")]
        role: Option<String>,
        /// Ownership relation field used when --access owner is selected.
        #[arg(long, conflicts_with = "role")]
        owner: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum PullAccess {
    None,
    Public,
    Authenticated,
    Role,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PullMode {
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
    preview: preview::PullPreview,
}

#[derive(Serialize)]
struct PullComparisonResult<'path> {
    command: &'static str,
    action: &'static str,
    schema: String,
    output: &'path Path,
    entity_count: usize,
    warnings: Vec<String>,
    preview: preview::PullPreview,
    current: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
}

#[derive(Clone, Copy)]
struct PullOptions<'argument> {
    schema: &'argument str,
    output: &'argument Path,
    mode: PullMode,
    review: bool,
    access: PullAccess,
    role: Option<&'argument str>,
    owner: Option<&'argument str>,
}

struct PreparedPull {
    output_path: PathBuf,
    schema: String,
    draft: render::Draft,
}

pub(crate) fn run(project: &Path, command: &DbCommand) -> ExitCode {
    match command {
        DbCommand::Pull {
            schema,
            output,
            check,
            diff,
            review,
            access,
            role,
            owner,
        } => pull(
            project,
            PullOptions {
                schema,
                output,
                mode: if *check {
                    PullMode::Check
                } else if *diff {
                    PullMode::Diff
                } else {
                    PullMode::Create
                },
                review: *review,
                access: *access,
                role: role.as_deref(),
                owner: owner.as_deref(),
            },
        ),
    }
}

fn pull(project: &Path, options: PullOptions<'_>) -> ExitCode {
    let prepared = match prepare_pull(project, options) {
        Ok(Some(prepared)) => prepared,
        Ok(None) => return ExitCode::SUCCESS,
        Err(exit) => return exit,
    };
    if options.mode != PullMode::Create {
        return compare_draft(
            project,
            &prepared.output_path,
            prepared.schema,
            prepared.draft,
            options.mode,
        );
    }
    if let Some(parent) = prepared.output_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return write_error(&prepared.output_path, &error);
    }
    if let Err(error) =
        crate::transaction::write_new(&prepared.output_path, prepared.draft.source.as_bytes())
    {
        return write_error(&prepared.output_path, &error);
    }
    let relative = prepared
        .output_path
        .strip_prefix(project)
        .unwrap_or(&prepared.output_path);
    if crate::report::is_json() {
        let preview = preview::build(&prepared.draft);
        crate::report::success(&PullResult {
            command: "db",
            action: "pull",
            schema: prepared.schema,
            output: relative,
            entity_count: prepared.draft.entity_count,
            warnings: prepared.draft.warnings,
            preview,
        });
    } else {
        println!(
            "Created App Spec draft {} ({} entities)",
            relative.display(),
            prepared.draft.entity_count
        );
        preview::print(&prepared.draft);
        for warning in &prepared.draft.warnings {
            crate::report::warning("AS6306", crate::report::ErrorCategory::Database, warning);
        }
        println!("Review access rules, then add the draft to appstruct.yaml includes");
    }
    ExitCode::SUCCESS
}

fn prepare_pull(
    project: &Path,
    options: PullOptions<'_>,
) -> Result<Option<PreparedPull>, ExitCode> {
    if options.review && (crate::report::is_json() || !review::interactive()) {
        return Err(crate::report::fail(
            "AS6310",
            crate::report::ErrorCategory::Project,
            "db pull --review requires a terminal and text output",
            crate::report::ExitClass::Usage,
        ));
    }
    if let Err(message) = validate_schema_name(options.schema) {
        return Err(crate::report::fail(
            "AS6301",
            crate::report::ErrorCategory::Configuration,
            message,
            crate::report::ExitClass::Usage,
        ));
    }
    let configured_access = match resolve_access(options.access, options.role, options.owner) {
        Ok(access) => access,
        Err(message) => {
            return Err(crate::report::fail(
                "AS6312",
                crate::report::ErrorCategory::Configuration,
                message,
                crate::report::ExitClass::Usage,
            ));
        }
    };
    let output_path = match output::resolve(project, options.output, options.mode) {
        Ok(path) => path,
        Err(error) => {
            return Err(crate::report::fail(
                "AS6302",
                crate::report::ErrorCategory::Project,
                error.to_string(),
                crate::report::ExitClass::Usage,
            ));
        }
    };
    let environment = match crate::environment::ProjectEnvironment::load(project) {
        Ok(environment) => environment,
        Err(error) => {
            return Err(crate::report::fail(
                "AS6303",
                crate::report::ErrorCategory::Configuration,
                format!("cannot load project environment: {error}"),
                crate::report::ExitClass::Environment,
            ));
        }
    };
    let Some(database_url) = environment.get("DATABASE_URL") else {
        return Err(crate::report::fail(
            "AS6304",
            crate::report::ErrorCategory::Configuration,
            "DATABASE_URL is required for db pull",
            crate::report::ExitClass::Environment,
        ));
    };
    let inspection = match inspect_database_schema(&database_url, options.schema) {
        Ok(inspection) => inspection,
        Err(error) => {
            return Err(crate::report::fail(
                "AS6305",
                crate::report::ErrorCategory::Database,
                error.to_string(),
                crate::report::ExitClass::Database,
            ));
        }
    };
    let (inspection, access) = if options.review {
        match review_schema(inspection) {
            Ok(Some((selected, selected_access))) => (selected, selected_access),
            Ok(None) => return Ok(None),
            Err(exit) => return Err(exit),
        }
    } else {
        (inspection, configured_access)
    };
    let draft = render::render_with_access(&inspection, &access);
    Ok(Some(PreparedPull {
        output_path,
        schema: inspection.name,
        draft,
    }))
}

fn review_schema(
    inspection: appstruct_migrate::IntrospectedSchema,
) -> Result<Option<(appstruct_migrate::IntrospectedSchema, render::AccessMode)>, ExitCode> {
    match review::select(inspection) {
        Ok(Some((selected, access))) => Ok(Some((selected, access))),
        Ok(None) => {
            println!("Import cancelled; no draft was written");
            Ok(None)
        }
        Err(error) => Err(crate::report::fail(
            "AS6311",
            crate::report::ErrorCategory::Project,
            format!("cannot review database tables: {error}"),
            crate::report::ExitClass::Usage,
        )),
    }
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
        let preview = preview::build(&draft);
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
            preview,
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
