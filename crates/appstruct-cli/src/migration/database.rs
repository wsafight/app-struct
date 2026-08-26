use appstruct_migrate::{
    ApplyReport, DriftStatus, MigrationError, MigrationStatus, apply_project, status_project,
};
use std::env;
use std::path::Path;
use std::process::ExitCode;

pub(super) fn apply(project: &Path, configured_url: Option<&str>) -> ExitCode {
    let Some(database_url) = database_url(configured_url) else {
        eprintln!("error[AS4107]: DATABASE_URL is required for migrate apply");
        return ExitCode::from(3);
    };
    apply_with_url(project, &database_url)
}

pub(super) fn apply_if_configured(
    project: &Path,
    configured_url: Option<&str>,
) -> Option<ExitCode> {
    database_url(configured_url).map(|database_url| apply_with_url(project, &database_url))
}

pub(super) fn status(project: &Path, configured_url: Option<&str>) -> ExitCode {
    let Some(database_url) = database_url(configured_url) else {
        eprintln!("error[AS4107]: DATABASE_URL is required for migrate status");
        return ExitCode::from(3);
    };
    match status_project(project, &database_url) {
        Ok(status) => render_status(&status),
        Err(error) => render_error(&error),
    }
}

fn apply_with_url(project: &Path, database_url: &str) -> ExitCode {
    match apply_project(project, database_url) {
        Ok(report) => render_apply(&report),
        Err(error) => render_error(&error),
    }
}

fn database_url(configured_url: Option<&str>) -> Option<String> {
    configured_url
        .map(str::to_owned)
        .or_else(|| env::var("DATABASE_URL").ok())
        .filter(|value| !value.trim().is_empty())
}

fn render_apply(report: &ApplyReport) -> ExitCode {
    println!(
        "Applied {} migration(s); {} total",
        report.applied_now, report.total_applied
    );
    render_drift(&report.drift)
}

fn render_status(status: &MigrationStatus) -> ExitCode {
    println!("Migration status:");
    println!("- applied: {}", status.applied);
    println!("- pending: {}", status.pending);
    render_drift(&status.drift)
}

fn render_drift(drift: &DriftStatus) -> ExitCode {
    match drift {
        DriftStatus::Clean => {
            println!("- drift: none");
            ExitCode::SUCCESS
        }
        DriftStatus::Deferred => {
            println!("- drift: deferred until pending migrations are applied");
            ExitCode::SUCCESS
        }
        DriftStatus::Detected(issues) => {
            eprintln!("error[AS4111]: database schema drift detected");
            for issue in issues {
                eprintln!("- {issue}");
            }
            ExitCode::from(4)
        }
    }
}

fn render_error(error: &MigrationError) -> ExitCode {
    let (code, exit) = match error {
        MigrationError::Project(_) => ("AS4108", 1),
        MigrationError::Database(_) => ("AS4109", 4),
        MigrationError::Integrity(_) => ("AS4110", 4),
    };
    eprintln!("error[{code}]: {error}");
    ExitCode::from(exit)
}
