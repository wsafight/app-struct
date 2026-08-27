use appstruct_migrate::{
    ApplyReport, DriftStatus, MigrationError, MigrationStatus, apply_project, status_project,
};
use std::env;
use std::path::Path;
use std::process::ExitCode;

pub(super) fn apply(project: &Path, configured_url: Option<&str>) -> ExitCode {
    let Some(database_url) = database_url(configured_url) else {
        return crate::report::fail(
            "AS4107",
            crate::report::ErrorCategory::Configuration,
            "DATABASE_URL is required for migrate apply",
            crate::report::ExitClass::Environment,
        );
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
        return crate::report::fail(
            "AS4107",
            crate::report::ErrorCategory::Configuration,
            "DATABASE_URL is required for migrate status",
            crate::report::ExitClass::Environment,
        );
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
    if matches!(&report.drift, DriftStatus::Detected(_)) {
        return render_drift(&report.drift);
    }
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "migrate",
            "action": "apply",
            "applied_now": report.applied_now,
            "total_applied": report.total_applied,
            "drift": drift_name(&report.drift),
        }));
    } else {
        println!(
            "Applied {} migration(s); {} total",
            report.applied_now, report.total_applied
        );
    }
    render_drift(&report.drift)
}

fn render_status(status: &MigrationStatus) -> ExitCode {
    if matches!(&status.drift, DriftStatus::Detected(_)) {
        return render_drift(&status.drift);
    }
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "migrate",
            "action": "status",
            "applied": status.applied,
            "pending": status.pending,
            "drift": drift_name(&status.drift),
        }));
    } else {
        println!("Migration status:");
        println!("- applied: {}", status.applied);
        println!("- pending: {}", status.pending);
    }
    render_drift(&status.drift)
}

fn render_drift(drift: &DriftStatus) -> ExitCode {
    match drift {
        DriftStatus::Clean => {
            if !crate::report::is_json() {
                println!("- drift: none");
            }
            ExitCode::SUCCESS
        }
        DriftStatus::Deferred => {
            if !crate::report::is_json() {
                println!("- drift: deferred until pending migrations are applied");
            }
            ExitCode::SUCCESS
        }
        DriftStatus::Detected(issues) => crate::report::fail(
            "AS4111",
            crate::report::ErrorCategory::Database,
            format!("database schema drift detected: {}", issues.join("; ")),
            crate::report::ExitClass::Database,
        ),
    }
}

fn drift_name(drift: &DriftStatus) -> &'static str {
    match drift {
        DriftStatus::Clean => "clean",
        DriftStatus::Deferred => "deferred",
        DriftStatus::Detected(_) => "detected",
    }
}

fn render_error(error: &MigrationError) -> ExitCode {
    let (code, exit) = match error {
        MigrationError::Project(_) => ("AS4108", crate::report::ExitClass::Validation),
        MigrationError::Database(_) => ("AS4109", crate::report::ExitClass::Database),
        MigrationError::Integrity(_) => ("AS4110", crate::report::ExitClass::Database),
    };
    crate::report::fail(
        code,
        crate::report::ErrorCategory::Database,
        error.to_string(),
        exit,
    )
}
