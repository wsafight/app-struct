use super::ownership;
use crate::transaction::{
    JOURNAL_VERSION, RecoveryJournal, TransactionFault, TransactionLock, TransactionPhase,
    read_latest, remove_dir_all, remove_file, rename, sync_tree,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(crate) use crate::transaction::TransactionFault as GenerationFault;

const LOCK_NAME: &str = "generation.lock";
const JOURNAL_NAME: &str = "generation.journal";
const STAGING_NAME: &str = ".generated.appstruct-staging";
const BACKUP_NAME: &str = ".generated.appstruct-backup";

pub(crate) struct GenerationTransaction {
    paths: TransactionPaths,
    _lock: TransactionLock,
}

struct TransactionPaths {
    root: PathBuf,
    staging: PathBuf,
    backup: PathBuf,
    journal: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct JournalRecord {
    version: u32,
    phase: TransactionPhase,
}

impl crate::transaction::JournalRecord for JournalRecord {
    fn version(&self) -> u32 {
        self.version
    }
}

impl GenerationTransaction {
    pub(crate) fn acquire(project: &Path) -> io::Result<Self> {
        Self::acquire_inner(project, false)
    }

    pub(crate) fn acquire_for_update(project: &Path) -> io::Result<Self> {
        Self::acquire_inner(project, true)
    }

    fn acquire_inner(project: &Path, allow_update_recovery: bool) -> io::Result<Self> {
        let state = project.join(".appstruct");
        fs::create_dir_all(&state)?;
        let lock_path = state.join(LOCK_NAME);
        let lock = TransactionLock::acquire(&lock_path, "generation")?;
        if !allow_update_recovery && update_state_exists(project) {
            return Err(invalid(
                "an unfinished project update exists; run `appstruct update` to recover it",
            ));
        }
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

    pub(crate) fn replace(&self, files: &BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
        self.replace_inner(files, TransactionFault::Disabled)
    }

    #[cfg(test)]
    pub(crate) fn replace_with_fault(
        &self,
        files: &BTreeMap<PathBuf, Vec<u8>>,
        fault: TransactionFault,
    ) -> io::Result<()> {
        self.replace_inner(files, fault)
    }

    fn replace_inner(
        &self,
        files: &BTreeMap<PathBuf, Vec<u8>>,
        fault: GenerationFault,
    ) -> io::Result<()> {
        if self.paths.staging.exists() || self.paths.backup.exists() || self.paths.journal.exists()
        {
            return Err(invalid(
                "generated directory recovery did not reach a clean state",
            ));
        }
        if let Err(error) = write_staging(&self.paths.staging, &self.paths.root, files) {
            let _ = fs::remove_dir_all(&self.paths.staging);
            return Err(error);
        }
        if let Err(error) = preserve_cargo_locks(&self.paths.root, &self.paths.staging) {
            let _ = fs::remove_dir_all(&self.paths.staging);
            return Err(error);
        }
        if let Err(error) = ownership::validate_owned_tree(&self.paths.staging) {
            let _ = remove_dir_all(&self.paths.staging);
            return Err(error);
        }
        sync_tree(&self.paths.staging)?;
        if let Err(error) = self.swap(fault) {
            return match self.recover() {
                Ok(()) => Err(error),
                Err(recovery) => Err(invalid(format!(
                    "{error}; automatic recovery also failed: {recovery}"
                ))),
            };
        }
        Ok(())
    }

    fn swap(&self, fault: TransactionFault) -> io::Result<()> {
        let mut record = JournalRecord {
            version: JOURNAL_VERSION,
            phase: TransactionPhase::Prepared,
        };
        let mut journal = RecoveryJournal::start(&self.paths.journal, &record)?;
        fault.check(TransactionPhase::Prepared, "generation")?;
        if self.paths.root.exists() {
            rename(&self.paths.root, &self.paths.backup)?;
        }
        record.phase = TransactionPhase::BackedUp;
        journal.record(&record)?;
        fault.check(TransactionPhase::BackedUp, "generation")?;
        rename(&self.paths.staging, &self.paths.root)?;
        record.phase = TransactionPhase::Installed;
        journal.record(&record)?;
        fault.check(TransactionPhase::Installed, "generation")?;
        if self.paths.backup.exists() {
            remove_dir_all(&self.paths.backup)?;
        }
        journal.finish()
    }

    fn recover(&self) -> io::Result<()> {
        let record = read_latest::<JournalRecord>(&self.paths.journal, "generation")?;
        match record.map(|record| record.phase) {
            Some(TransactionPhase::Prepared) => self.recover_prepared()?,
            Some(TransactionPhase::BackedUp) => self.recover_backed_up()?,
            Some(TransactionPhase::Installed) => self.recover_installed()?,
            None => self.recover_without_journal()?,
        }
        remove_file(&self.paths.journal)
    }

    fn recover_prepared(&self) -> io::Result<()> {
        if self.paths.backup.exists() {
            ownership::validate_owned_tree(&self.paths.backup)?;
            if self.paths.root.exists() {
                ownership::validate_owned_tree(&self.paths.root)?;
                remove_dir_all(&self.paths.root)?;
            }
            rename(&self.paths.backup, &self.paths.root)?;
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
                remove_dir_all(&self.paths.backup)
            }
            (false, true) => {
                ownership::validate_owned_tree(&self.paths.backup)?;
                self.remove_staging()?;
                rename(&self.paths.backup, &self.paths.root)
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
            remove_dir_all(&self.paths.backup)?;
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
                remove_dir_all(&self.paths.backup)?;
            } else {
                rename(&self.paths.backup, &self.paths.root)?;
            }
        }
        if staging {
            if self.paths.backup.exists() {
                return Err(ambiguous());
            }
            remove_dir_all(&self.paths.staging)?;
        }
        Ok(())
    }

    fn remove_staging(&self) -> io::Result<()> {
        if self.paths.staging.exists() {
            ownership::validate_owned_tree(&self.paths.staging)?;
            remove_dir_all(&self.paths.staging)?;
        }
        Ok(())
    }
}

fn update_state_exists(project: &Path) -> bool {
    [
        project.join(".appstruct/update.journal"),
        project.join(".generated.appstruct-update-staging"),
        project.join(".generated.appstruct-update-backup"),
        project.join(".appstruct.lock.appstruct-update-staging"),
        project.join(".appstruct.lock.appstruct-update-backup"),
    ]
    .iter()
    .any(|path| path.exists())
}

fn write_staging(
    staging: &Path,
    current: &Path,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> io::Result<()> {
    fs::create_dir(staging)?;
    for (relative, content) in files {
        let path = staging.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| invalid("artifact has no parent"))?;
        fs::create_dir_all(parent)?;
        let previous = current.join(relative);
        if fs::read(&previous).is_ok_and(|bytes| bytes == *content)
            && fs::hard_link(&previous, &path).is_ok()
        {
            continue;
        }
        let mut file = File::create(path)?;
        file.write_all(content)?;
        file.sync_all()?;
    }
    Ok(())
}

fn preserve_cargo_locks(root: &Path, staging: &Path) -> io::Result<()> {
    for package in ["backend", "server"] {
        let source = root.join(package).join("Cargo.lock");
        let manifest_unchanged = fs::read(root.join(package).join("Cargo.toml"))
            .ok()
            .zip(fs::read(staging.join(package).join("Cargo.toml")).ok())
            .is_some_and(|(old, new)| old == new);
        if source.is_file() && manifest_unchanged {
            fs::copy(source, staging.join(package).join("Cargo.lock"))?;
        }
    }
    Ok(())
}

fn ambiguous() -> io::Error {
    invalid("ambiguous generated directory recovery state; preserving all trees")
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
