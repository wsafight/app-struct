use super::ownership;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const LOCK_NAME: &str = "generation.lock";
const JOURNAL_NAME: &str = "generation.journal";
const STAGING_NAME: &str = ".generated.appstruct-staging";
const BACKUP_NAME: &str = ".generated.appstruct-backup";

pub(super) struct GenerationTransaction {
    paths: TransactionPaths,
    _lock: File,
}

struct TransactionPaths {
    root: PathBuf,
    staging: PathBuf,
    backup: PathBuf,
    journal: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    BackedUp,
    Installed,
}

#[derive(Debug, Deserialize, Serialize)]
struct JournalRecord {
    version: u32,
    phase: Phase,
}

impl GenerationTransaction {
    pub(super) fn acquire(project: &Path) -> io::Result<Self> {
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
                "another generation process holds `{}`",
                lock_path.display()
            )),
            TryLockError::Error(error) => error,
        })?;
        let transaction = Self {
            paths: TransactionPaths {
                root: project.join("generated"),
                staging: project.join(STAGING_NAME),
                backup: project.join(BACKUP_NAME),
                journal: state.join(JOURNAL_NAME),
            },
            _lock: lock,
        };
        transaction.recover()?;
        Ok(transaction)
    }

    pub(super) fn replace(&self, files: &BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
        if self.paths.staging.exists() || self.paths.backup.exists() || self.paths.journal.exists()
        {
            return Err(invalid(
                "generated directory recovery did not reach a clean state",
            ));
        }
        if let Err(error) = write_staging(&self.paths.staging, files) {
            let _ = fs::remove_dir_all(&self.paths.staging);
            return Err(error);
        }
        if let Err(error) = preserve_cargo_lock(&self.paths.root, &self.paths.staging) {
            let _ = fs::remove_dir_all(&self.paths.staging);
            return Err(error);
        }
        if let Err(error) = ownership::validate_owned_tree(&self.paths.staging) {
            let _ = fs::remove_dir_all(&self.paths.staging);
            return Err(error);
        }
        if let Err(error) = self.swap() {
            return match self.recover() {
                Ok(()) => Err(error),
                Err(recovery) => Err(invalid(format!(
                    "{error}; automatic recovery also failed: {recovery}"
                ))),
            };
        }
        Ok(())
    }

    fn swap(&self) -> io::Result<()> {
        let mut journal = RecoveryJournal::start(&self.paths.journal)?;
        if self.paths.root.exists() {
            fs::rename(&self.paths.root, &self.paths.backup)?;
        }
        journal.record(Phase::BackedUp)?;
        fs::rename(&self.paths.staging, &self.paths.root)?;
        journal.record(Phase::Installed)?;
        if self.paths.backup.exists() {
            fs::remove_dir_all(&self.paths.backup)?;
        }
        journal.finish()
    }

    fn recover(&self) -> io::Result<()> {
        let phase = read_phase(&self.paths.journal)?;
        match phase {
            Some(Phase::Prepared) => self.recover_prepared()?,
            Some(Phase::BackedUp) => self.recover_backed_up()?,
            Some(Phase::Installed) => self.recover_installed()?,
            None => self.recover_without_journal()?,
        }
        remove_file_if_exists(&self.paths.journal)
    }

    fn recover_prepared(&self) -> io::Result<()> {
        if self.paths.backup.exists() {
            ownership::validate_owned_tree(&self.paths.backup)?;
            if self.paths.root.exists() {
                ownership::validate_owned_tree(&self.paths.root)?;
                fs::remove_dir_all(&self.paths.root)?;
            }
            fs::rename(&self.paths.backup, &self.paths.root)?;
        }
        self.remove_staging()
    }

    fn recover_backed_up(&self) -> io::Result<()> {
        match (self.paths.root.exists(), self.paths.backup.exists()) {
            (true, true) => {
                ownership::validate_owned_tree(&self.paths.root)?;
                ownership::validate_owned_tree(&self.paths.backup)?;
                if self.paths.staging.exists() {
                    return Err(ambiguous());
                }
                fs::remove_dir_all(&self.paths.backup)
            }
            (false, true) => {
                ownership::validate_owned_tree(&self.paths.backup)?;
                self.remove_staging()?;
                fs::rename(&self.paths.backup, &self.paths.root)
            }
            (true, false) => {
                if self.paths.staging.exists() {
                    return Err(ambiguous());
                }
                ownership::validate_owned_tree(&self.paths.root)
            }
            (false, false) => self.remove_staging(),
        }
    }

    fn recover_installed(&self) -> io::Result<()> {
        if !self.paths.root.exists() {
            return Err(invalid("installed generated directory is missing"));
        }
        ownership::validate_owned_tree(&self.paths.root)?;
        if self.paths.staging.exists() {
            return Err(ambiguous());
        }
        if self.paths.backup.exists() {
            ownership::validate_owned_tree(&self.paths.backup)?;
            fs::remove_dir_all(&self.paths.backup)?;
        }
        Ok(())
    }

    fn recover_without_journal(&self) -> io::Result<()> {
        let root = self.paths.root.exists();
        let staging = self.paths.staging.exists();
        let backup = self.paths.backup.exists();
        if root && staging && backup {
            return Err(ambiguous());
        }
        if backup {
            ownership::validate_owned_tree(&self.paths.backup)?;
            if root {
                ownership::validate_owned_tree(&self.paths.root)?;
                fs::remove_dir_all(&self.paths.backup)?;
            } else {
                fs::rename(&self.paths.backup, &self.paths.root)?;
            }
        }
        if staging {
            if self.paths.backup.exists() {
                return Err(ambiguous());
            }
            fs::remove_dir_all(&self.paths.staging)?;
        }
        Ok(())
    }

    fn remove_staging(&self) -> io::Result<()> {
        if self.paths.staging.exists() {
            ownership::validate_owned_tree(&self.paths.staging)?;
            fs::remove_dir_all(&self.paths.staging)?;
        }
        Ok(())
    }
}

struct RecoveryJournal {
    path: PathBuf,
    file: File,
}

impl RecoveryJournal {
    fn start(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(path)?;
        let mut journal = Self {
            path: path.to_path_buf(),
            file,
        };
        journal.record(Phase::Prepared)?;
        Ok(journal)
    }

    fn record(&mut self, phase: Phase) -> io::Result<()> {
        let record = JournalRecord { version: 1, phase };
        serde_json::to_writer(&mut self.file, &record).map_err(io::Error::other)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.file.sync_all()
    }

    fn finish(self) -> io::Result<()> {
        drop(self.file);
        fs::remove_file(self.path)
    }
}

fn read_phase(path: &Path) -> io::Result<Option<Phase>> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut phase = None;
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<JournalRecord>(line) else {
            break;
        };
        if record.version != 1 {
            return Err(invalid(format!(
                "unsupported generation journal version {}",
                record.version
            )));
        }
        phase = Some(record.phase);
    }
    Ok(phase)
}

fn write_staging(staging: &Path, files: &BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
    fs::create_dir(staging)?;
    for (relative, content) in files {
        let path = staging.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| invalid("artifact has no parent"))?;
        fs::create_dir_all(parent)?;
        let mut file = File::create(path)?;
        file.write_all(content)?;
        file.sync_all()?;
    }
    Ok(())
}

fn preserve_cargo_lock(root: &Path, staging: &Path) -> io::Result<()> {
    let source = root.join("backend/Cargo.lock");
    let manifest_unchanged = fs::read(root.join("backend/Cargo.toml"))
        .ok()
        .zip(fs::read(staging.join("backend/Cargo.toml")).ok())
        .is_some_and(|(old, new)| old == new);
    if source.is_file() && manifest_unchanged {
        fs::copy(source, staging.join("backend/Cargo.lock"))?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn ambiguous() -> io::Error {
    invalid("ambiguous generated directory recovery state; preserving all trees")
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
