use super::{
    DriftStatus, MigrationError, ProjectMigrations, catalog, connect_database, history, reconcile,
    stamp_schema_checksum,
};
use crate::{DatabaseSchema, initial_migration, to_json};
use appstruct_schema::sql::quote_ident;
use postgres::GenericClient;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineStatus {
    pub migration: Option<String>,
    pub registered: bool,
    pub drift: DriftStatus,
}

/// Compare a proposed application's structure against one schema using a read-only transaction.
/// This does not validate or register migration history.
///
/// # Errors
///
/// Returns connection or catalog errors. No project files, schema objects or business rows change.
pub fn inspect_schema_structure(
    database_url: &str,
    schema: &str,
    expected: &DatabaseSchema,
) -> Result<DriftStatus, MigrationError> {
    let mut client = connect_database(database_url)?;
    let mut transaction = client
        .build_transaction()
        .read_only(true)
        .start()
        .map_err(|error| database_error(&error))?;
    transaction
        .query_one(
            "SELECT set_config('search_path', $1, true)",
            &[&quote_ident(schema)],
        )
        .map_err(|error| database_error(&error))?;
    let issues = catalog::detect(&mut transaction, expected)?;
    Ok(if issues.is_empty() {
        DriftStatus::Clean
    } else {
        DriftStatus::Detected(issues)
    })
}

/// Preview adoption of an existing database without writing files or database history.
///
/// # Errors
///
/// Rejects seeds, empty schemas, existing unrelated migrations, invalid history, and database
/// failures. Schema differences are returned in `drift`.
pub fn inspect_baseline(
    project: &Path,
    database_url: &str,
    expected: &DatabaseSchema,
) -> Result<BaselineStatus, MigrationError> {
    let migrations = validate_project(project, expected)?;
    let mut client = connect_database(database_url)?;
    let applied = history::load(&mut client)?;
    let reconciliation = reconcile(&migrations.files, &applied)?;
    let issues = catalog::detect(&mut client, expected)?;
    Ok(BaselineStatus {
        migration: migrations.files.first().map(|file| file.id.clone()),
        registered: reconciliation.applied == 1,
        drift: if issues.is_empty() {
            DriftStatus::Clean
        } else {
            DriftStatus::Detected(issues)
        },
    })
}

/// Register one canonical initial migration without executing its SQL or changing business rows.
///
/// # Errors
///
/// Requires a nonempty schema without seeds, a matching canonical migration and snapshot, clean
/// history, and matching live structure. Catalog validation and history registration run in one
/// transaction while holding table locks and the migration runner's advisory lock.
pub fn baseline_project(
    project: &Path,
    database_url: &str,
    expected: &DatabaseSchema,
) -> Result<BaselineStatus, MigrationError> {
    let migrations = validate_project(project, expected)?;
    let migration = migrations.files.first().ok_or_else(|| {
        MigrationError::Project(
            "create the initial baseline migration before registering it".to_owned(),
        )
    })?;
    let mut client = connect_database(database_url)?;
    history::lock(&mut client)?;
    let mut transaction = client
        .transaction()
        .map_err(|error| database_error(&error))?;
    let applied = history::load(&mut transaction)?;
    reconcile(&migrations.files, &applied)?;
    require_matching_schema(&mut transaction, expected)?;
    // Block concurrent schema changes while allowing normal row reads and writes. Qualify names
    // so a table in a different search_path schema cannot be locked instead.
    let schema: String = transaction
        .query_one("SELECT current_schema()", &[])
        .and_then(|row| row.try_get(0))
        .map_err(|error| database_error(&error))?;
    let tables = expected
        .tables
        .iter()
        .map(|table| format!("{}.{}", quote_ident(&schema), quote_ident(&table.name)))
        .collect::<Vec<_>>()
        .join(", ");
    transaction
        .batch_execute(&format!(
            "SET LOCAL lock_timeout = '5s'; LOCK TABLE {tables} IN SHARE UPDATE EXCLUSIVE MODE"
        ))
        .map_err(|error| database_error(&error))?;
    require_matching_schema(&mut transaction, expected)?;
    if applied.is_empty() {
        history::ensure_table(&mut transaction)?;
        transaction
            .execute(
                r#"INSERT INTO "_appstruct_migrations"
    (migration_id, checksum, state, finished_at)
VALUES ($1, $2, 'applied', CURRENT_TIMESTAMP)"#,
                &[&migration.id, &migration.checksum],
            )
            .map_err(|error| database_error(&error))?;
    }
    transaction
        .commit()
        .map_err(|error| database_error(&error))?;
    Ok(BaselineStatus {
        migration: Some(migration.id.clone()),
        registered: true,
        drift: DriftStatus::Clean,
    })
}

fn validate_project(
    project: &Path,
    expected: &DatabaseSchema,
) -> Result<ProjectMigrations, MigrationError> {
    if expected.tables.is_empty() || !expected.seeds.is_empty() {
        return Err(MigrationError::Project(
            "baseline requires a nonempty schema without seeds; handle seed data explicitly after adoption".to_owned(),
        ));
    }
    let migrations = ProjectMigrations::load(project)?;
    if migrations.files.is_empty() {
        if project.join(".appstruct/schema.snapshot.json").exists() {
            return Err(MigrationError::Project(
                "baseline cannot replace an existing snapshot".to_owned(),
            ));
        }
        return Ok(migrations);
    }
    let snapshot = to_json(expected).map_err(|error| MigrationError::Project(error.to_string()))?;
    let canonical = stamp_schema_checksum(&initial_migration(expected), &snapshot);
    if migrations.files.len() != 1 || migrations.files[0].sql != canonical {
        return Err(MigrationError::Project(
            "baseline accepts only one unchanged canonical initial migration matching the current Spec; it cannot skip arbitrary SQL or existing migration chains".to_owned(),
        ));
    }
    Ok(migrations)
}

fn require_matching_schema(
    client: &mut impl GenericClient,
    expected: &DatabaseSchema,
) -> Result<(), MigrationError> {
    let issues = catalog::detect(client, expected)?;
    if issues.is_empty() {
        Ok(())
    } else {
        Err(MigrationError::Integrity(format!(
            "database does not match the baseline: {}",
            issues.join("; ")
        )))
    }
}

fn database_error(error: &postgres::Error) -> MigrationError {
    MigrationError::Database(format!(
        "cannot register migration baseline: {}",
        super::database_message(error)
    ))
}
