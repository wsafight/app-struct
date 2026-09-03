use appstruct_migrate::inspect_database_schema;
use clap::Subcommand;
use serde::Serialize;
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
    },
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

pub(crate) fn run(project: &Path, command: &DbCommand) -> ExitCode {
    match command {
        DbCommand::Pull { schema, output } => pull(project, schema, output),
    }
}

fn pull(project: &Path, schema: &str, output: &Path) -> ExitCode {
    if let Err(message) = validate_schema_name(schema) {
        return crate::report::fail(
            "AS6301",
            crate::report::ErrorCategory::Configuration,
            message,
            crate::report::ExitClass::Usage,
        );
    }
    let output_path = match resolve_output(project, output) {
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

fn resolve_output(project: &Path, output: &Path) -> std::io::Result<PathBuf> {
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
    if fs::symlink_metadata(&path).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("db pull output `{}` already exists", path.display()),
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_paths_are_project_relative_and_never_overwritten() {
        let project = tempfile::tempdir().unwrap();
        assert!(resolve_output(project.path(), Path::new("spec/imported.yaml")).is_ok());
        assert!(resolve_output(project.path(), Path::new("../outside.yaml")).is_err());
        assert!(resolve_output(project.path(), Path::new("spec/imported.json")).is_err());
        fs::create_dir(project.path().join("spec")).unwrap();
        fs::write(project.path().join("spec/imported.yaml"), "keep\n").unwrap();
        assert!(resolve_output(project.path(), Path::new("spec/imported.yaml")).is_err());
    }

    #[test]
    fn schema_names_reject_ambiguous_values() {
        assert!(validate_schema_name("public").is_ok());
        assert!(validate_schema_name("").is_err());
        assert!(validate_schema_name(" public").is_err());
        assert!(validate_schema_name(&"x".repeat(64)).is_err());
        assert!(validate_schema_name("pub\nlic").is_err());
    }

    #[test]
    fn resolve_output_rejects_symlinked_and_file_parents() {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("file.yaml"), "x\n").unwrap();
        assert!(resolve_output(project.path(), Path::new("/tmp/imported.yaml")).is_err());
        fs::write(project.path().join("not-a-dir"), "x\n").unwrap();
        assert!(resolve_output(project.path(), Path::new("not-a-dir/imported.yaml")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("not-a-dir", project.path().join("link-dir")).unwrap();
            assert!(resolve_output(project.path(), Path::new("link-dir/imported.yaml")).is_err());
        }
    }

    #[test]
    fn pull_rejects_invalid_schema_names_and_output_paths() {
        let project = tempfile::tempdir().unwrap();
        assert_ne!(
            run(
                project.path(),
                &DbCommand::Pull {
                    schema: String::new(),
                    output: PathBuf::from("spec/imported.yaml"),
                },
            ),
            ExitCode::SUCCESS
        );
        assert_ne!(
            run(
                project.path(),
                &DbCommand::Pull {
                    schema: "public".to_owned(),
                    output: PathBuf::from("../outside.yaml"),
                },
            ),
            ExitCode::SUCCESS
        );
        assert_ne!(
            run(
                project.path(),
                &DbCommand::Pull {
                    schema: "public".to_owned(),
                    output: PathBuf::from("spec/imported.yaml"),
                },
            ),
            ExitCode::SUCCESS
        );
    }
}
