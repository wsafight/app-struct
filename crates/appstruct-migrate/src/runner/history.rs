use super::{MigrationError, MigrationFile};
use postgres::Client;

const HISTORY_TABLE: &str = "_appstruct_migrations";

pub(super) struct HistoryEntry {
    pub id: String,
    pub checksum: String,
    pub state: String,
}

pub(super) fn lock(client: &mut Client) -> Result<(), MigrationError> {
    client
        .simple_query("SELECT pg_advisory_lock(hashtext('appstruct:migrations'))")
        .map(|_| ())
        .map_err(|error| MigrationError::Database(format!("cannot lock migration runner: {error}")))
}

pub(super) fn ensure_table(client: &mut Client) -> Result<(), MigrationError> {
    client
        .batch_execute(
            r#"CREATE TABLE IF NOT EXISTS "_appstruct_migrations" (
    migration_id TEXT PRIMARY KEY,
    checksum TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('applying', 'applied', 'failed')),
    applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at TIMESTAMPTZ
)"#,
        )
        .map_err(|error| {
            MigrationError::Database(format!("cannot create migration history table: {error}"))
        })
}

pub(super) fn load(client: &mut Client) -> Result<Vec<HistoryEntry>, MigrationError> {
    if !exists(client)? {
        return Ok(Vec::new());
    }
    client
        .query(
            r#"SELECT migration_id, checksum, state
FROM "_appstruct_migrations"
ORDER BY migration_id"#,
            &[],
        )
        .map_err(|error| {
            MigrationError::Database(format!("cannot read migration history: {error}"))
        })?
        .into_iter()
        .map(|row| {
            Ok(HistoryEntry {
                id: row
                    .try_get("migration_id")
                    .map_err(|error| history_decode_error(&error))?,
                checksum: row
                    .try_get("checksum")
                    .map_err(|error| history_decode_error(&error))?,
                state: row
                    .try_get("state")
                    .map_err(|error| history_decode_error(&error))?,
            })
        })
        .collect()
}

pub(super) fn apply(client: &mut Client, migration: &MigrationFile) -> Result<(), MigrationError> {
    if migration.transactional {
        apply_transactional(client, migration)
    } else {
        apply_without_transaction(client, migration)
    }
}

fn exists(client: &mut Client) -> Result<bool, MigrationError> {
    client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1)",
            &[&HISTORY_TABLE],
        )
        .and_then(|row| row.try_get(0))
        .map_err(|error| {
            MigrationError::Database(format!("cannot inspect migration history: {error}"))
        })
}

fn apply_transactional(
    client: &mut Client,
    migration: &MigrationFile,
) -> Result<(), MigrationError> {
    let mut transaction = client
        .transaction()
        .map_err(|error| database_error(migration, "cannot start migration transaction", &error))?;
    transaction
        .batch_execute(&migration.sql)
        .map_err(|error| database_error(migration, "migration SQL failed", &error))?;
    transaction
        .execute(
            r#"INSERT INTO "_appstruct_migrations"
    (migration_id, checksum, state, finished_at)
VALUES ($1, $2, 'applied', CURRENT_TIMESTAMP)"#,
            &[&migration.id, &migration.checksum],
        )
        .map_err(|error| database_error(migration, "cannot record migration", &error))?;
    transaction
        .commit()
        .map_err(|error| database_error(migration, "cannot commit migration", &error))
}

fn apply_without_transaction(
    client: &mut Client,
    migration: &MigrationFile,
) -> Result<(), MigrationError> {
    client
        .execute(
            r#"INSERT INTO "_appstruct_migrations" (migration_id, checksum, state)
VALUES ($1, $2, 'applying')"#,
            &[&migration.id, &migration.checksum],
        )
        .map_err(|error| database_error(migration, "cannot mark migration as applying", &error))?;
    if let Err(error) = client.batch_execute(&migration.sql) {
        let _ = client.execute(
            r#"UPDATE "_appstruct_migrations"
SET state = 'failed', finished_at = CURRENT_TIMESTAMP
WHERE migration_id = $1"#,
            &[&migration.id],
        );
        return Err(database_error(
            migration,
            "non-transactional migration failed",
            &error,
        ));
    }
    client
        .execute(
            r#"UPDATE "_appstruct_migrations"
SET state = 'applied', finished_at = CURRENT_TIMESTAMP
WHERE migration_id = $1"#,
            &[&migration.id],
        )
        .map(|_| ())
        .map_err(|error| database_error(migration, "cannot finalize migration history", &error))
}

fn database_error(
    migration: &MigrationFile,
    context: &str,
    error: &postgres::Error,
) -> MigrationError {
    MigrationError::Database(format!(
        "{context} for `{}`: {}",
        migration.id,
        super::database_message(error)
    ))
}

fn history_decode_error(error: &postgres::Error) -> MigrationError {
    MigrationError::Database(format!(
        "invalid migration history row: {}",
        super::database_message(error)
    ))
}
