use crate::transaction::{
    JOURNAL_VERSION, JournalRecord, RecoveryJournal, TransactionFault, TransactionLock,
    TransactionPhase, invalid, read_latest, remove_file, rename, write_new,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const JOURNAL_NAME: &str = "migration.journal";
const LOCK_NAME: &str = "migration.lock";
const SNAPSHOT_NAME: &str = "schema.snapshot.json";
const SNAPSHOT_STAGING_NAME: &str = "schema.snapshot.json.tmp";
const SNAPSHOT_BACKUP_NAME: &str = "schema.snapshot.json.appstruct-backup";

pub(super) struct MigrationTransaction {
    paths: MigrationPaths,
    _lock: TransactionLock,
}

struct MigrationPaths {
    migrations: PathBuf,
    state: PathBuf,
    snapshot: PathBuf,
    snapshot_staging: PathBuf,
    snapshot_backup: PathBuf,
    journal: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MigrationRecord {
    version: u32,
    phase: TransactionPhase,
    migration: String,
    had_snapshot: bool,
}

impl JournalRecord for MigrationRecord {
    fn version(&self) -> u32 {
        self.version
    }
}

impl MigrationTransaction {
    pub(super) fn acquire(project: &Path) -> io::Result<Self> {
        let paths = MigrationPaths::new(project);
        fs::create_dir_all(&paths.migrations)?;
        fs::create_dir_all(&paths.state)?;
        let lock = TransactionLock::acquire(&paths.state.join(LOCK_NAME), "migration")?;
        let transaction = Self { paths, _lock: lock };
        transaction.recover()?;
        Ok(transaction)
    }

    pub(super) fn commit(&self, sql: &str, snapshot: &str) -> io::Result<PathBuf> {
        self.commit_inner(sql, snapshot, TransactionFault::Disabled)
    }

    #[cfg(test)]
    pub(super) fn commit_with_fault(
        &self,
        sql: &str,
        snapshot: &str,
        fault: TransactionFault,
    ) -> io::Result<PathBuf> {
        self.commit_inner(sql, snapshot, fault)
    }

    fn commit_inner(
        &self,
        sql: &str,
        snapshot: &str,
        fault: TransactionFault,
    ) -> io::Result<PathBuf> {
        self.ensure_clean()?;
        let migration = next_migration_name(&self.paths.migrations)?;
        let final_path = self.paths.migrations.join(&migration);
        let staging = migration_staging(&final_path);
        if final_path.exists() || staging.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("migration target `{}` already exists", final_path.display()),
            ));
        }
        let mut record = MigrationRecord {
            version: JOURNAL_VERSION,
            phase: TransactionPhase::Prepared,
            migration,
            had_snapshot: self.paths.snapshot.is_file(),
        };
        match self.swap(&final_path, &staging, &mut record, sql, snapshot, fault) {
            Ok(()) => Ok(final_path),
            Err(error) => self.recover_after(error),
        }
    }

    fn swap(
        &self,
        final_path: &Path,
        staging: &Path,
        record: &mut MigrationRecord,
        sql: &str,
        snapshot: &str,
        fault: TransactionFault,
    ) -> io::Result<()> {
        let mut journal = RecoveryJournal::start(&self.paths.journal, &*record)?;
        write_new(staging, sql.as_bytes())?;
        write_new(&self.paths.snapshot_staging, snapshot.as_bytes())?;
        fault.check(TransactionPhase::Prepared, "migration")?;
        if record.had_snapshot {
            rename(&self.paths.snapshot, &self.paths.snapshot_backup)?;
        }
        record.phase = TransactionPhase::BackedUp;
        journal.record(&*record)?;
        fault.check(TransactionPhase::BackedUp, "migration")?;
        rename(staging, final_path)?;
        rename(&self.paths.snapshot_staging, &self.paths.snapshot)?;
        record.phase = TransactionPhase::Installed;
        journal.record(&*record)?;
        fault.check(TransactionPhase::Installed, "migration")?;
        remove_file(&self.paths.snapshot_backup)?;
        journal.finish()
    }

    fn recover_after<T>(&self, error: io::Error) -> io::Result<T> {
        match self.recover() {
            Ok(()) => Err(error),
            Err(recovery) => Err(invalid(format!(
                "{error}; automatic migration recovery also failed: {recovery}"
            ))),
        }
    }

    fn recover(&self) -> io::Result<()> {
        let record = read_latest::<MigrationRecord>(&self.paths.journal, "migration")?;
        match record {
            Some(record) => {
                Self::validate_record(&record)?;
                match record.phase {
                    TransactionPhase::Prepared => self.recover_prepared(&record)?,
                    TransactionPhase::BackedUp => self.recover_backed_up(&record)?,
                    TransactionPhase::Installed => self.recover_installed(&record)?,
                }
            }
            None => self.recover_without_journal()?,
        }
        remove_file(&self.paths.journal)
    }

    fn recover_prepared(&self, record: &MigrationRecord) -> io::Result<()> {
        let final_path = self.paths.migrations.join(&record.migration);
        if final_path.exists() {
            return Err(ambiguous());
        }
        if self.paths.snapshot_backup.exists() {
            if self.paths.snapshot.exists() {
                return Err(ambiguous());
            }
            rename(&self.paths.snapshot_backup, &self.paths.snapshot)?;
        } else if record.had_snapshot && !self.paths.snapshot.exists() {
            return Err(invalid(
                "schema snapshot disappeared during migration recovery",
            ));
        }
        self.remove_staging(record)
    }

    fn recover_backed_up(&self, record: &MigrationRecord) -> io::Result<()> {
        let final_path = self.paths.migrations.join(&record.migration);
        if final_path.exists() && !final_path.is_file() {
            return Err(ambiguous());
        }
        remove_file(&final_path)?;
        remove_file(&self.paths.snapshot)?;
        if record.had_snapshot {
            if !self.paths.snapshot_backup.is_file() {
                return Err(invalid("schema snapshot backup is missing"));
            }
            rename(&self.paths.snapshot_backup, &self.paths.snapshot)?;
        } else if self.paths.snapshot_backup.exists() {
            return Err(ambiguous());
        }
        self.remove_staging(record)
    }

    fn recover_installed(&self, record: &MigrationRecord) -> io::Result<()> {
        let final_path = self.paths.migrations.join(&record.migration);
        if !final_path.is_file() || !self.paths.snapshot.is_file() {
            return Err(invalid("installed migration transaction is incomplete"));
        }
        if migration_staging(&final_path).exists() || self.paths.snapshot_staging.exists() {
            return Err(ambiguous());
        }
        remove_file(&self.paths.snapshot_backup)
    }

    fn recover_without_journal(&self) -> io::Result<()> {
        if has_migration_staging(&self.paths.migrations)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "migration staging file exists without a recovery journal; preserving it",
            ));
        }
        if self.paths.snapshot_staging.exists() || self.paths.snapshot_backup.exists() {
            return Err(ambiguous());
        }
        Ok(())
    }

    fn remove_staging(&self, record: &MigrationRecord) -> io::Result<()> {
        remove_file(&migration_staging(
            &self.paths.migrations.join(&record.migration),
        ))?;
        remove_file(&self.paths.snapshot_staging)
    }

    fn ensure_clean(&self) -> io::Result<()> {
        if self.paths.journal.exists()
            || self.paths.snapshot_staging.exists()
            || self.paths.snapshot_backup.exists()
            || has_migration_staging(&self.paths.migrations)?
        {
            return Err(invalid(
                "migration recovery did not reach a clean transaction state",
            ));
        }
        Ok(())
    }

    fn validate_record(record: &MigrationRecord) -> io::Result<()> {
        let path = Path::new(&record.migration);
        if path.components().count() != 1
            || !matches!(path.components().next(), Some(Component::Normal(_)))
            || path.extension().is_none_or(|extension| extension != "sql")
        {
            return Err(invalid("migration journal contains an unsafe filename"));
        }
        Ok(())
    }
}

impl MigrationPaths {
    fn new(project: &Path) -> Self {
        let state = project.join(".appstruct");
        Self {
            migrations: project.join("migrations"),
            snapshot: state.join(SNAPSHOT_NAME),
            snapshot_staging: state.join(SNAPSHOT_STAGING_NAME),
            snapshot_backup: state.join(SNAPSHOT_BACKUP_NAME),
            journal: state.join(JOURNAL_NAME),
            state,
        }
    }
}

pub(super) fn next_migration_name(directory: &Path) -> io::Result<String> {
    let mut last = 0_u32;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|value| value != "sql") {
            continue;
        }
        let Some(sequence) = path
            .file_stem()
            .and_then(|name| name.to_str())
            .and_then(|name| name.split('_').next())
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        last = last.max(sequence);
    }
    let next = last
        .checked_add(1)
        .ok_or_else(|| io::Error::other("migration sequence exhausted"))?;
    Ok(format!("{next:04}_appstruct.sql"))
}

fn migration_staging(path: &Path) -> PathBuf {
    path.with_extension("sql.tmp")
}

fn has_migration_staging(directory: &Path) -> io::Result<bool> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "tmp")
            && path
                .file_stem()
                .and_then(|name| Path::new(name).extension())
                .is_some_and(|extension| extension == "sql")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ambiguous() -> io::Error {
    invalid("ambiguous migration recovery state; preserving all transaction paths")
}
