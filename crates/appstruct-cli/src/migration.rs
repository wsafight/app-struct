use appstruct_ir::DatabaseProvider;
use appstruct_migrate::{
    DatabaseSchema, LintSeverity, MigrationPlan, SCHEMA_VERSION, SchemaChange, diff, extract,
    from_json, lint_plan, migration_sql, stamp_schema_checksum, to_json,
};
use clap::Subcommand;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod transaction;

use transaction::MigrationTransaction;

mod database;
mod development;
#[cfg(test)]
mod tests;

pub(crate) use development::prepare_development;

#[derive(Clone, Copy, Debug, Subcommand)]
pub(crate) enum MigrateCommand {
    /// Show the schema diff without writing files or connecting to a database.
    Plan,
    /// Accept a safe development migration and update the schema snapshot.
    Dev {
        /// Confirm creation of the migration and snapshot in non-interactive environments.
        #[arg(long)]
        accept: bool,
    },
    /// Apply pending migration files to the configured PostgreSQL database.
    Apply,
    /// Compare migration files, database history, and the live PostgreSQL schema.
    Status,
    /// Check the migration plan for unsafe or operationally risky changes.
    Lint {
        /// Treat warnings as errors.
        #[arg(long)]
        deny_warnings: bool,
    },
}

pub(crate) fn run(project: &Path, command: MigrateCommand) -> ExitCode {
    run_with_database(project, command, None)
}

pub(crate) fn run_with_database(
    project: &Path,
    command: MigrateCommand,
    database_url: Option<&str>,
) -> ExitCode {
    match command {
        MigrateCommand::Apply => return database::apply(project, database_url),
        MigrateCommand::Status => return database::status(project, database_url),
        MigrateCommand::Plan | MigrateCommand::Dev { .. } | MigrateCommand::Lint { .. } => {}
    }
    let target = match appstruct_compiler::compile_project(project) {
        Ok(ir) => match extract(&ir) {
            Ok(schema) => schema,
            Err(error) => {
                return crate::report::fail(
                    "AS4101",
                    crate::report::ErrorCategory::Migration,
                    error.to_string(),
                    crate::report::ExitClass::Validation,
                );
            }
        },
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let before = match read_snapshot(project) {
        Ok(Some(schema)) => schema,
        Ok(None) => empty_schema(),
        Err(error) => {
            return crate::report::fail(
                "AS4101",
                crate::report::ErrorCategory::Migration,
                format!("cannot read schema snapshot: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    };
    let plan = diff(&before, &target);
    if !crate::report::is_json()
        && matches!(command, MigrateCommand::Plan | MigrateCommand::Dev { .. })
    {
        render_plan(&plan);
    }
    match command {
        MigrateCommand::Plan => {
            if crate::report::is_json() {
                crate::report::success(&serde_json::json!({
                    "command": "migrate",
                    "action": "plan",
                    "blocked": plan.is_blocked(),
                    "changes": plan.changes,
                }));
            }
            ExitCode::SUCCESS
        }
        MigrateCommand::Dev { accept } => {
            accept_plan(project, &target, &plan, accept, database_url)
        }
        MigrateCommand::Lint { deny_warnings } => render_lint(&plan, deny_warnings),
        MigrateCommand::Apply | MigrateCommand::Status => unreachable!(),
    }
}

fn render_lint(plan: &MigrationPlan, deny_warnings: bool) -> ExitCode {
    let issues = lint_plan(plan);
    let denied = deny_warnings
        && issues
            .iter()
            .any(|issue| issue.severity == LintSeverity::Warning);
    let has_errors = issues
        .iter()
        .any(|issue| issue.severity == LintSeverity::Error);
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "migrate",
            "action": "lint",
            "valid": !has_errors && !denied,
            "issues": issues,
        }));
    } else if issues.is_empty() {
        println!("Migration lint: no issues");
    } else {
        println!("Migration lint ({} issues):", issues.len());
        for issue in &issues {
            println!("- [{}] {}", issue.code, issue.message);
        }
    }
    if has_errors || denied {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn accept_plan(
    project: &Path,
    target: &DatabaseSchema,
    plan: &MigrationPlan,
    accept: bool,
    database_url: Option<&str>,
) -> ExitCode {
    if plan.is_empty() {
        if !crate::report::is_json() {
            println!("Schema snapshot is current");
        }
        return database::apply_if_configured(project, database_url).unwrap_or_else(|| {
            render_dev_success(None, true, false);
            ExitCode::SUCCESS
        });
    }
    if plan.is_blocked() {
        return crate::report::fail(
            "AS4102",
            crate::report::ErrorCategory::Migration,
            "migration contains destructive or review-required changes",
            crate::report::ExitClass::Validation,
        );
    }
    if !accept {
        return crate::report::fail(
            "AS4103",
            crate::report::ErrorCategory::Migration,
            "pass `--accept` to create this development migration",
            crate::report::ExitClass::Usage,
        );
    }
    let sql = match migration_sql(plan) {
        Ok(sql) => sql,
        Err(error) => {
            return crate::report::fail(
                "AS4104",
                crate::report::ErrorCategory::Migration,
                format!("cannot render migration: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    };
    let snapshot = match to_json(target) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return crate::report::fail(
                "AS4105",
                crate::report::ErrorCategory::Migration,
                format!("cannot serialize schema snapshot: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    };
    let sql = stamp_schema_checksum(&sql, &snapshot);
    match write_plan(project, &sql, &snapshot) {
        Ok(path) => {
            if !crate::report::is_json() {
                println!("Created safe migration {}", path.display());
            }
            database::apply_if_configured(project, database_url).unwrap_or_else(|| {
                if !crate::report::is_json() {
                    println!("Migration is pending; set DATABASE_URL to apply it");
                }
                render_dev_success(Some(&path), false, true);
                ExitCode::SUCCESS
            })
        }
        Err(error) => crate::report::fail(
            "AS4106",
            crate::report::ErrorCategory::Transaction,
            format!("cannot commit migration plan: {error}"),
            crate::report::ExitClass::Environment,
        ),
    }
}

fn render_dev_success(path: Option<&Path>, current: bool, pending: bool) {
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "migrate",
            "action": "dev",
            "current": current,
            "migration": path,
            "pending": pending,
        }));
    }
}

fn read_snapshot(project: &Path) -> io::Result<Option<DatabaseSchema>> {
    let path = project.join(".appstruct/schema.snapshot.json");
    match fs::read_to_string(path) {
        Ok(source) => from_json(&source)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn empty_schema() -> DatabaseSchema {
    DatabaseSchema {
        schema_version: SCHEMA_VERSION,
        provider: DatabaseProvider::Postgres,
        tables: Vec::new(),
        unique_constraints: Vec::new(),
        indexes: Vec::new(),
        seeds: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

fn write_plan(project: &Path, sql: &str, snapshot: &str) -> io::Result<PathBuf> {
    MigrationTransaction::acquire(project)?.commit(sql, snapshot)
}

fn render_plan(plan: &MigrationPlan) {
    if plan.is_empty() {
        println!("Migration plan: no changes");
        return;
    }
    println!("Migration plan ({} changes):", plan.changes.len());
    for planned in &plan.changes {
        println!(
            "- {} [{:?}/{:?}]",
            change_label(&planned.change),
            planned.risk.schema,
            planned.risk.execution
        );
    }
}

fn change_label(change: &SchemaChange) -> String {
    match change {
        SchemaChange::AddTable { table } => format!("add table `{}`", table.name),
        SchemaChange::RemoveTable { table } => format!("remove table `{}`", table.name),
        SchemaChange::RenameTable { before, after } => {
            format!("rename table `{}` to `{}`", before.name, after.name)
        }
        SchemaChange::AddColumn { table, column } => {
            format!("add column `{table}.{}`", column.name)
        }
        SchemaChange::RemoveColumn { table, column } => {
            format!("remove column `{table}.{}`", column.name)
        }
        SchemaChange::AlterColumn { table, after, .. } => {
            format!("alter column `{table}.{}`", after.name)
        }
        SchemaChange::AddUniqueConstraint { constraint } => {
            format!("add unique constraint `{}`", constraint.id)
        }
        SchemaChange::RemoveUniqueConstraint { constraint } => {
            format!("remove unique constraint `{}`", constraint.id)
        }
        SchemaChange::AddIndex { index } => format!("add index `{}`", index.id),
        SchemaChange::RemoveIndex { index } => format!("remove index `{}`", index.id),
        SchemaChange::AddSeed { seed } => format!("add seed `{}`", seed.id),
        SchemaChange::RemoveSeed { seed } => format!("remove seed `{}`", seed.id),
        SchemaChange::AddForeignKey { foreign_key } => {
            format!("add foreign key `{}`", foreign_key.id)
        }
        SchemaChange::RemoveForeignKey { foreign_key } => {
            format!("remove foreign key `{}`", foreign_key.id)
        }
    }
}
