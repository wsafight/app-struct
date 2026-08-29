use super::{MigrateCommand, empty_schema, read_snapshot, render_plan, run_with_database};
use appstruct_ir::DatabaseMigrationPolicy;
use appstruct_migrate::{
    DriftStatus, MigrationPlan, MigrationStatus, diff, extract, status_project,
};
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

pub(crate) fn prepare_development(
    project: &Path,
    database_url: &str,
    policy: DatabaseMigrationPolicy,
    announce_unmanaged: bool,
) -> io::Result<()> {
    match policy {
        DatabaseMigrationPolicy::Auto => apply_accepted(project, database_url),
        DatabaseMigrationPolicy::Prompt => prompt(project, database_url),
        DatabaseMigrationPolicy::Never => validate_current(project, database_url),
        DatabaseMigrationPolicy::Unmanaged => {
            if announce_unmanaged {
                eprintln!(
                    "[appstruct] database migrations are externally managed; schema compatibility was not checked"
                );
            }
            Ok(())
        }
    }
}

fn prompt(project: &Path, database_url: &str) -> io::Result<()> {
    let state = inspect(project, database_url)?;
    if state.plan.is_empty() && state.status.pending == 0 {
        return Ok(());
    }
    render_plan(&state.plan);
    if state.status.pending > 0 {
        println!(
            "Pending migrations: {} file(s) have not been applied",
            state.status.pending
        );
    }
    if state.plan.is_blocked() {
        return Err(io::Error::other(
            "migration contains destructive or review-required changes",
        ));
    }
    if !confirm()? {
        return Err(io::Error::other(
            "database migration was not approved; update the database or choose another `database.dev.migration` policy",
        ));
    }
    apply_accepted(project, database_url)
}

fn validate_current(project: &Path, database_url: &str) -> io::Result<()> {
    let state = inspect(project, database_url)?;
    let planned = state.plan.changes.len();
    if planned == 0 && state.status.pending == 0 {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "database migration policy is `never`, but {planned} schema change(s) and {} pending migration(s) require attention",
        state.status.pending
    )))
}

struct DevelopmentState {
    plan: appstruct_migrate::MigrationPlan,
    status: MigrationStatus,
}

fn inspect(project: &Path, database_url: &str) -> io::Result<DevelopmentState> {
    let plan = development_plan(project).map_err(io::Error::other)?;
    let status = status_project(project, database_url)
        .map_err(|error| io::Error::other(format!("cannot inspect migration state: {error}")))?;
    match &status.drift {
        DriftStatus::Clean | DriftStatus::Deferred if status.pending > 0 => {}
        DriftStatus::Clean => {}
        DriftStatus::Deferred => {
            return Err(io::Error::other(
                "database drift inspection was unexpectedly deferred",
            ));
        }
        DriftStatus::Detected(issues) => {
            return Err(io::Error::other(format!(
                "database schema drift detected: {}",
                issues.join("; ")
            )));
        }
    }
    Ok(DevelopmentState { plan, status })
}

fn development_plan(project: &Path) -> Result<MigrationPlan, String> {
    let ir = appstruct_compiler::compile_project(project).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let target = extract(&ir).map_err(|error| error.to_string())?;
    let before = read_snapshot(project)
        .map_err(|error| format!("cannot read schema snapshot: {error}"))?
        .unwrap_or_else(empty_schema);
    Ok(diff(&before, &target))
}

fn confirm() -> io::Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::other(
            "database migration policy `prompt` requires an interactive terminal; run `appstruct migrate dev --accept` or configure `auto`, `never`, or `unmanaged`",
        ));
    }
    print!("Apply database migrations? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer)? == 0 {
        return Ok(false);
    }
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn apply_accepted(project: &Path, database_url: &str) -> io::Result<()> {
    if run_with_database(
        project,
        MigrateCommand::Dev { accept: true },
        Some(database_url),
    ) == ExitCode::SUCCESS
    {
        Ok(())
    } else {
        Err(io::Error::other("migration failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmanaged_never_reads_project_or_database_state() {
        prepare_development(
            Path::new("/missing/project"),
            "not-a-database-url",
            DatabaseMigrationPolicy::Unmanaged,
            false,
        )
        .unwrap();
    }
}
