use super::{MigrationTransaction, database};
use appstruct_migrate::{
    BaselineStatus, DatabaseSchema, DriftStatus, MigrationError, baseline_project,
    initial_migration, inspect_baseline, stamp_schema_checksum, to_json,
};
use std::io;
use std::path::Path;
use std::process::ExitCode;

pub(super) fn run(
    project: &Path,
    schema: &DatabaseSchema,
    accept: bool,
    configured_url: Option<&str>,
) -> ExitCode {
    let Some(url) = database::database_url(configured_url) else {
        return crate::report::fail(
            "AS4107",
            crate::report::ErrorCategory::Configuration,
            "DATABASE_URL is required for migrate baseline",
            crate::report::ExitClass::Environment,
        );
    };
    // Hold the filesystem migration lock across inspection, file creation, and DB registration.
    // The default preview does not acquire it, so it creates no directories or recovery files.
    let transaction = if accept {
        match MigrationTransaction::acquire(project) {
            Ok(transaction) => Some(transaction),
            Err(error) => return file_error(&error),
        }
    } else {
        None
    };
    let status = match inspect_baseline(project, &url, schema) {
        Ok(status) => status,
        Err(error) => return database::render_error(&error),
    };
    if !matches!(status.drift, DriftStatus::Clean) {
        return database::render_drift(&status.drift);
    }
    let Some(transaction) = transaction else {
        return render(&status, false);
    };
    if status.migration.is_none() {
        let snapshot = match to_json(schema) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return database::render_error(&MigrationError::Project(error.to_string()));
            }
        };
        let sql = stamp_schema_checksum(&initial_migration(schema), &snapshot);
        if let Err(error) = transaction.commit(&sql, &snapshot) {
            return file_error(&error);
        }
    }
    match baseline_project(project, &url, schema) {
        Ok(status) => render(&status, true),
        Err(error) => {
            let message = format!(
                "{error}; baseline files were retained; resolve the error and retry `migrate baseline --accept`"
            );
            let error = match error {
                MigrationError::Project(_) => MigrationError::Project(message),
                MigrationError::Database(_) => MigrationError::Database(message),
                MigrationError::Integrity(_) => MigrationError::Integrity(message),
            };
            database::render_error(&error)
        }
    }
}

fn render(status: &BaselineStatus, accepted: bool) -> ExitCode {
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "migrate",
            "action": "baseline",
            "accepted": accepted,
            "registered": status.registered,
            "migration": status.migration,
            "drift": "clean",
        }));
    } else if accepted {
        println!("Baseline registered; no business SQL executed");
    } else {
        println!("Database structure matches the current Spec; no changes made");
        println!("Run `appstruct migrate baseline --accept` to register the initial migration");
    }
    ExitCode::SUCCESS
}

fn file_error(error: &io::Error) -> ExitCode {
    crate::report::fail(
        "AS4106",
        crate::report::ErrorCategory::Transaction,
        format!("cannot prepare migration baseline: {error}"),
        crate::report::ExitClass::Environment,
    )
}
