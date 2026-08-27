use crate::generation::{ownership, transaction::GenerationTransaction};
use crate::transaction::{
    JOURNAL_VERSION, RecoveryJournal, TransactionFault, TransactionLock, TransactionPhase,
    read_latest, remove_dir_all, remove_file, rename, sync_tree, write_new,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const JOURNAL_NAME: &str = "update.journal";
const LOCK_NAME: &str = "update.lock";
const GENERATED_STAGING: &str = ".generated.appstruct-update-staging";
const GENERATED_BACKUP: &str = ".generated.appstruct-update-backup";
const LOCK_STAGING: &str = ".appstruct.lock.appstruct-update-staging";
const LOCK_BACKUP: &str = ".appstruct.lock.appstruct-update-backup";

pub(super) struct UpdateTransaction {
    paths: UpdatePaths,
    _generation: GenerationTransaction,
    _lock: TransactionLock,
}

struct UpdatePaths {
    generated: PathBuf,
    generated_staging: PathBuf,
    generated_backup: PathBuf,
    lock: PathBuf,
    lock_staging: PathBuf,
    lock_backup: PathBuf,
    journal: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct JournalRecord {
    version: u32,
    phase: TransactionPhase,
    had_lock: bool,
    had_generated: bool,
}

impl crate::transaction::JournalRecord for JournalRecord {
    fn version(&self) -> u32 {
        self.version
    }
}

impl UpdateTransaction {
    pub(super) fn acquire(project: &Path) -> io::Result<Self> {
        let generation = GenerationTransaction::acquire_for_update(project)?;
        let state = project.join(".appstruct");
        fs::create_dir_all(&state)?;
        let lock_path = state.join(LOCK_NAME);
        let lock = TransactionLock::acquire(&lock_path, "update")?;
        let transaction = Self {
            paths: UpdatePaths::new(project, &state),
            _generation: generation,
            _lock: lock,
        };
        transaction.recover()?;
        Ok(transaction)
    }

    pub(super) fn commit(&self, candidate: &Path, lock: &[u8]) -> io::Result<()> {
        self.commit_inner(candidate, lock, TransactionFault::Disabled)
    }

    #[cfg(test)]
    pub(super) fn commit_with_fault(
        &self,
        candidate: &Path,
        lock: &[u8],
        fault: TransactionFault,
    ) -> io::Result<()> {
        self.commit_inner(candidate, lock, fault)
    }

    fn commit_inner(
        &self,
        candidate: &Path,
        lock: &[u8],
        fault: TransactionFault,
    ) -> io::Result<()> {
        self.ensure_clean()?;
        if self.paths.generated.exists() {
            ownership::validate_owned_tree(&self.paths.generated)?;
        }
        if let Err(error) = ownership::copy_owned_tree(candidate, &self.paths.generated_staging) {
            self.remove_staging_best_effort();
            return Err(error);
        }
        sync_tree(&self.paths.generated_staging)?;
        if let Err(error) = write_new(&self.paths.lock_staging, lock) {
            self.remove_staging_best_effort();
            return Err(error);
        }
        let record = JournalRecord {
            version: JOURNAL_VERSION,
            phase: TransactionPhase::Prepared,
            had_lock: self.paths.lock.is_file(),
            had_generated: self.paths.generated.is_dir(),
        };
        if let Err(error) = self.swap(record, fault) {
            return match self.recover() {
                Ok(()) => Err(error),
                Err(recovery) => Err(invalid(format!(
                    "{error}; automatic update recovery also failed: {recovery}"
                ))),
            };
        }
        Ok(())
    }

    fn swap(&self, mut record: JournalRecord, fault: TransactionFault) -> io::Result<()> {
        let mut journal = RecoveryJournal::start(&self.paths.journal, &record)?;
        fault.check(TransactionPhase::Prepared, "update")?;
        rename_if_exists(&self.paths.lock, &self.paths.lock_backup)?;
        rename_if_exists(&self.paths.generated, &self.paths.generated_backup)?;
        record.phase = TransactionPhase::BackedUp;
        journal.record(&record)?;
        fault.check(TransactionPhase::BackedUp, "update")?;
        rename(&self.paths.lock_staging, &self.paths.lock)?;
        rename(&self.paths.generated_staging, &self.paths.generated)?;
        record.phase = TransactionPhase::Installed;
        journal.record(&record)?;
        fault.check(TransactionPhase::Installed, "update")?;
        self.remove_backups()?;
        journal.finish()
    }

    fn recover(&self) -> io::Result<()> {
        match read_latest::<JournalRecord>(&self.paths.journal, "update")? {
            Some(record) => match record.phase {
                TransactionPhase::Prepared => self.recover_prepared(record)?,
                TransactionPhase::BackedUp => self.recover_backed_up(record)?,
                TransactionPhase::Installed => self.recover_installed()?,
            },
            None => self.recover_without_journal()?,
        }
        remove_file(&self.paths.journal)
    }

    fn recover_prepared(&self, record: JournalRecord) -> io::Result<()> {
        restore_if_backed_up(&self.paths.lock, &self.paths.lock_backup, record.had_lock)?;
        restore_generated_if_backed_up(
            &self.paths.generated,
            &self.paths.generated_backup,
            record.had_generated,
        )?;
        self.remove_staging()
    }

    fn recover_backed_up(&self, record: JournalRecord) -> io::Result<()> {
        rollback_file(&self.paths.lock, &self.paths.lock_backup, record.had_lock)?;
        rollback_generated(
            &self.paths.generated,
            &self.paths.generated_backup,
            record.had_generated,
        )?;
        self.remove_staging()
    }

    fn recover_installed(&self) -> io::Result<()> {
        if !self.paths.lock.is_file() || !self.paths.generated.is_dir() {
            return Err(invalid("installed project update is incomplete"));
        }
        ownership::validate_owned_tree(&self.paths.generated)?;
        if self.paths.lock_staging.exists() || self.paths.generated_staging.exists() {
            return Err(ambiguous());
        }
        self.remove_backups()
    }

    fn recover_without_journal(&self) -> io::Result<()> {
        if self.paths.lock_backup.exists() || self.paths.generated_backup.exists() {
            return Err(ambiguous());
        }
        self.remove_staging()
    }

    fn ensure_clean(&self) -> io::Result<()> {
        if self.paths.lock_staging.exists()
            || self.paths.lock_backup.exists()
            || self.paths.generated_staging.exists()
            || self.paths.generated_backup.exists()
            || self.paths.journal.exists()
        {
            return Err(invalid(
                "project update recovery did not reach a clean state",
            ));
        }
        Ok(())
    }

    fn remove_staging(&self) -> io::Result<()> {
        remove_file(&self.paths.lock_staging)?;
        if self.paths.generated_staging.exists() {
            ownership::validate_owned_tree(&self.paths.generated_staging)?;
            remove_dir_all(&self.paths.generated_staging)?;
        }
        Ok(())
    }

    fn remove_staging_best_effort(&self) {
        let _ = self.remove_staging();
    }

    fn remove_backups(&self) -> io::Result<()> {
        remove_file(&self.paths.lock_backup)?;
        if self.paths.generated_backup.exists() {
            ownership::validate_owned_tree(&self.paths.generated_backup)?;
            remove_dir_all(&self.paths.generated_backup)?;
        }
        Ok(())
    }
}

impl UpdatePaths {
    fn new(project: &Path, state: &Path) -> Self {
        Self {
            generated: project.join("generated"),
            generated_staging: project.join(GENERATED_STAGING),
            generated_backup: project.join(GENERATED_BACKUP),
            lock: project.join("appstruct.lock"),
            lock_staging: project.join(LOCK_STAGING),
            lock_backup: project.join(LOCK_BACKUP),
            journal: state.join(JOURNAL_NAME),
        }
    }
}

fn restore_if_backed_up(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    if backup.exists() {
        if current.exists() {
            return Err(ambiguous());
        }
        rename(backup, current)?;
    } else if expected && !current.exists() {
        return Err(invalid("update lock disappeared during recovery"));
    }
    Ok(())
}

fn restore_generated_if_backed_up(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    if backup.exists() {
        ownership::validate_owned_tree(backup)?;
        if current.exists() {
            return Err(ambiguous());
        }
        rename(backup, current)?;
    } else if expected && !current.exists() {
        return Err(invalid("generated tree disappeared during update recovery"));
    }
    Ok(())
}

fn rollback_file(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    remove_file(current)?;
    if expected {
        if !backup.is_file() {
            return Err(invalid("update lock backup is missing"));
        }
        rename(backup, current)?;
    } else {
        remove_file(backup)?;
    }
    Ok(())
}

fn rollback_generated(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    if current.exists() {
        ownership::validate_owned_tree(current)?;
        remove_dir_all(current)?;
    }
    if expected {
        ownership::validate_owned_tree(backup)?;
        rename(backup, current)?;
    } else if backup.exists() {
        return Err(ambiguous());
    }
    Ok(())
}

fn rename_if_exists(source: &Path, destination: &Path) -> io::Result<()> {
    if source.exists() {
        rename(source, destination)?;
    }
    Ok(())
}

fn ambiguous() -> io::Error {
    invalid("ambiguous project update recovery state; preserving all paths")
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
