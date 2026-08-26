mod catalog;
mod history;
mod project;

use crate::DatabaseSchema;
use postgres::{Client, Config, NoTls, config::SslMode};
use postgres_native_tls::MakeTlsConnector;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;

use project::{MigrationFile, ProjectMigrations, reconcile};

const SCHEMA_CHECKSUM_PREFIX: &str = "-- appstruct:schema-sha256=";
const TRANSACTION_PREFIX: &str = "-- appstruct:transaction=";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriftStatus {
    Clean,
    Deferred,
    Detected(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationStatus {
    pub applied: usize,
    pub pending: usize,
    pub drift: DriftStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyReport {
    pub applied_now: usize,
    pub total_applied: usize,
    pub drift: DriftStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    Project(String),
    Database(String),
    Integrity(String),
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(message) | Self::Database(message) | Self::Integrity(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for MigrationError {}

#[must_use]
pub fn stamp_schema_checksum(sql: &str, snapshot: &str) -> String {
    let marker = format!(
        "{SCHEMA_CHECKSUM_PREFIX}{}\n",
        checksum(snapshot.as_bytes())
    );
    sql.split_once('\n').map_or_else(
        || format!("{sql}\n{marker}"),
        |(first, rest)| format!("{first}\n{marker}{rest}"),
    )
}

/// Inspect disk migrations, history, and database schema state.
///
/// # Errors
///
/// Returns an error for invalid project state, database failures, dirty history, or checksum
/// mismatches.
pub fn status_project(
    project: &Path,
    database_url: &str,
) -> Result<MigrationStatus, MigrationError> {
    let project = ProjectMigrations::load(project)?;
    let mut client = connect_database(database_url)?;
    let applied = history::load(&mut client)?;
    let reconciliation = reconcile(&project.files, &applied)?;
    let drift = if reconciliation.pending == 0 {
        detect_drift(&mut client, &project.schema)?
    } else {
        DriftStatus::Deferred
    };
    Ok(MigrationStatus {
        applied: reconciliation.applied,
        pending: reconciliation.pending,
        drift,
    })
}

/// Apply every pending disk migration and verify the resulting PostgreSQL schema.
///
/// # Errors
///
/// Returns an error for invalid project state, database failures, dirty history, or checksum
/// mismatches. Transaction-disabled failures remain recorded for manual recovery.
pub fn apply_project(project: &Path, database_url: &str) -> Result<ApplyReport, MigrationError> {
    let project = ProjectMigrations::load(project)?;
    let mut client = connect_database(database_url)?;
    history::lock(&mut client)?;
    history::ensure_table(&mut client)?;
    let applied = history::load(&mut client)?;
    let reconciliation = reconcile(&project.files, &applied)?;
    let pending_start = reconciliation.applied;
    for migration in &project.files[pending_start..] {
        history::apply(&mut client, migration)?;
    }
    let drift = detect_drift(&mut client, &project.schema)?;
    Ok(ApplyReport {
        applied_now: reconciliation.pending,
        total_applied: project.files.len(),
        drift,
    })
}

/// Connect to PostgreSQL using the same TLS policy as migration commands.
///
/// # Errors
///
/// Returns an error when the URL is invalid, TLS cannot be initialized, or PostgreSQL rejects the
/// connection.
pub fn connect_database(database_url: &str) -> Result<Client, MigrationError> {
    let config = database_url.parse::<Config>().map_err(|error| {
        MigrationError::Database(format!(
            "invalid PostgreSQL connection configuration: {error}"
        ))
    })?;
    let result = if config.get_ssl_mode() == SslMode::Disable {
        config.connect(NoTls)
    } else {
        let connector = native_tls::TlsConnector::builder()
            .build()
            .map_err(|error| {
                MigrationError::Database(format!("cannot initialize PostgreSQL TLS: {error}"))
            })?;
        config.connect(MakeTlsConnector::new(connector))
    };
    result.map_err(|error| {
        MigrationError::Database(format!(
            "cannot connect to PostgreSQL: {}",
            database_message(&error)
        ))
    })
}

fn database_message(error: &postgres::Error) -> String {
    error.as_db_error().map_or_else(
        || error.to_string(),
        |database| database.message().to_owned(),
    )
}

fn detect_drift(
    client: &mut Client,
    expected: &DatabaseSchema,
) -> Result<DriftStatus, MigrationError> {
    let issues = catalog::detect(client, expected)?;
    if issues.is_empty() {
        Ok(DriftStatus::Clean)
    } else {
        Ok(DriftStatus::Detected(issues))
    }
}

fn checksum(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

#[cfg(test)]
mod tests;
