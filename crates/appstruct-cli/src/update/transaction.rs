use crate::generation::{ownership, transaction::GenerationTransaction};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
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
    _lock: File,
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
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    BackedUp,
    Installed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct JournalRecord {
    version: u32,
    phase: Phase,
    had_lock: bool,
    had_generated: bool,
}

impl UpdateTransaction {
    pub(super) fn acquire(project: &Path) -> io::Result<Self> {
        let generation = GenerationTransaction::acquire_for_update(project)?;
        let state = project.join(".appstruct");
        fs::create_dir_all(&state)?;
        let lock_path = state.join(LOCK_NAME);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => invalid(format!(
                "another update process holds `{}`",
                lock_path.display()
            )),
            TryLockError::Error(error) => error,
        })?;
        let transaction = Self {
            paths: UpdatePaths::new(project, &state),
            _generation: generation,
            _lock: lock,
        };
        transaction.recover()?;
        Ok(transaction)
    }

    pub(super) fn commit(&self, candidate: &Path, lock: &[u8]) -> io::Result<()> {
        self.ensure_clean()?;
        if self.paths.generated.exists() {
            ownership::validate_owned_tree(&self.paths.generated)?;
        }
        if let Err(error) = ownership::copy_owned_tree(candidate, &self.paths.generated_staging) {
            self.remove_staging_best_effort();
            return Err(error);
        }
        if let Err(error) = write_file(&self.paths.lock_staging, lock) {
            self.remove_staging_best_effort();
            return Err(error);
        }
        let record = JournalRecord {
            version: 1,
            phase: Phase::Prepared,
            had_lock: self.paths.lock.is_file(),
            had_generated: self.paths.generated.is_dir(),
        };
        if let Err(error) = self.swap(record) {
            return match self.recover() {
                Ok(()) => Err(error),
                Err(recovery) => Err(invalid(format!(
                    "{error}; automatic update recovery also failed: {recovery}"
                ))),
            };
        }
        Ok(())
    }

    fn swap(&self, record: JournalRecord) -> io::Result<()> {
        let mut journal = RecoveryJournal::start(&self.paths.journal, record)?;
        rename_if_exists(&self.paths.lock, &self.paths.lock_backup)?;
        rename_if_exists(&self.paths.generated, &self.paths.generated_backup)?;
        journal.record(Phase::BackedUp)?;
        fs::rename(&self.paths.lock_staging, &self.paths.lock)?;
        fs::rename(&self.paths.generated_staging, &self.paths.generated)?;
        journal.record(Phase::Installed)?;
        self.remove_backups()?;
        journal.finish()
    }

    fn recover(&self) -> io::Result<()> {
        match read_record(&self.paths.journal)? {
            Some(record) => match record.phase {
                Phase::Prepared => self.recover_prepared(record)?,
                Phase::BackedUp => self.recover_backed_up(record)?,
                Phase::Installed => self.recover_installed()?,
            },
            None => self.recover_without_journal()?,
        }
        remove_file_if_exists(&self.paths.journal)
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
        remove_file_if_exists(&self.paths.lock_staging)?;
        if self.paths.generated_staging.exists() {
            ownership::validate_owned_tree(&self.paths.generated_staging)?;
            fs::remove_dir_all(&self.paths.generated_staging)?;
        }
        Ok(())
    }

    fn remove_staging_best_effort(&self) {
        let _ = self.remove_staging();
    }

    fn remove_backups(&self) -> io::Result<()> {
        remove_file_if_exists(&self.paths.lock_backup)?;
        if self.paths.generated_backup.exists() {
            ownership::validate_owned_tree(&self.paths.generated_backup)?;
            fs::remove_dir_all(&self.paths.generated_backup)?;
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

struct RecoveryJournal {
    path: PathBuf,
    file: File,
    record: JournalRecord,
}

impl RecoveryJournal {
    fn start(path: &Path, record: JournalRecord) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(path)?;
        let mut journal = Self {
            path: path.to_path_buf(),
            file,
            record,
        };
        journal.write().map(|()| journal)
    }

    fn record(&mut self, phase: Phase) -> io::Result<()> {
        self.record.phase = phase;
        self.write()
    }

    fn write(&mut self) -> io::Result<()> {
        serde_json::to_writer(&mut self.file, &self.record).map_err(io::Error::other)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.file.sync_all()
    }

    fn finish(self) -> io::Result<()> {
        drop(self.file);
        fs::remove_file(self.path)
    }
}

fn read_record(path: &Path) -> io::Result<Option<JournalRecord>> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut latest = None;
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<JournalRecord>(line) else {
            break;
        };
        if record.version != 1 {
            return Err(invalid(format!(
                "unsupported update journal version {}",
                record.version
            )));
        }
        latest = Some(record);
    }
    Ok(latest)
}

fn restore_if_backed_up(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    if backup.exists() {
        if current.exists() {
            return Err(ambiguous());
        }
        fs::rename(backup, current)?;
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
        fs::rename(backup, current)?;
    } else if expected && !current.exists() {
        return Err(invalid("generated tree disappeared during update recovery"));
    }
    Ok(())
}

fn rollback_file(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    remove_file_if_exists(current)?;
    if expected {
        if !backup.is_file() {
            return Err(invalid("update lock backup is missing"));
        }
        fs::rename(backup, current)?;
    } else {
        remove_file_if_exists(backup)?;
    }
    Ok(())
}

fn rollback_generated(current: &Path, backup: &Path, expected: bool) -> io::Result<()> {
    if current.exists() {
        ownership::validate_owned_tree(current)?;
        fs::remove_dir_all(current)?;
    }
    if expected {
        ownership::validate_owned_tree(backup)?;
        fs::rename(backup, current)?;
    } else if backup.exists() {
        return Err(ambiguous());
    }
    Ok(())
}

fn rename_if_exists(source: &Path, destination: &Path) -> io::Result<()> {
    if source.exists() {
        fs::rename(source, destination)?;
    }
    Ok(())
}

fn write_file(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(content)?;
    file.sync_all()
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn ambiguous() -> io::Error {
    invalid("ambiguous project update recovery state; preserving all paths")
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
