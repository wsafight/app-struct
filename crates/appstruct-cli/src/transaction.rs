use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(crate) const JOURNAL_VERSION: u32 = appstruct_contracts::TRANSACTION_JOURNAL.current;

#[derive(Clone, Copy, Debug, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TransactionPhase {
    Prepared,
    BackedUp,
    Installed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TransactionFault {
    Disabled,
    #[cfg(test)]
    AfterPrepared,
    #[cfg(test)]
    AfterBackup,
    #[cfg(test)]
    AfterInstall,
}

impl TransactionFault {
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn check(self, phase: TransactionPhase, operation: &str) -> io::Result<()> {
        #[cfg(test)]
        if matches!(
            (self, phase),
            (Self::AfterPrepared, TransactionPhase::Prepared)
                | (Self::AfterBackup, TransactionPhase::BackedUp)
                | (Self::AfterInstall, TransactionPhase::Installed)
        ) {
            return Err(io::Error::other(format!(
                "injected {operation} failure after {phase:?}"
            )));
        }
        let _ = (self, phase, operation);
        Ok(())
    }
}

pub(crate) struct TransactionLock {
    file: File,
}

impl TransactionLock {
    pub(crate) fn acquire(path: &Path, operation: &str) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => invalid(format!(
                "another {operation} process holds `{}`",
                path.display()
            )),
            TryLockError::Error(error) => error,
        })?;
        Ok(Self { file })
    }
}

impl Drop for TransactionLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(crate) struct RecoveryJournal {
    path: PathBuf,
    file: File,
}

impl RecoveryJournal {
    pub(crate) fn start<T: Serialize>(path: &Path, record: &T) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(path)?;
        let mut journal = Self {
            path: path.to_path_buf(),
            file,
        };
        journal.record(record)?;
        sync_parent(path)?;
        Ok(journal)
    }

    pub(crate) fn record<T: Serialize>(&mut self, record: &T) -> io::Result<()> {
        serde_json::to_writer(&mut self.file, record).map_err(io::Error::other)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.file.sync_all()
    }

    pub(crate) fn finish(self) -> io::Result<()> {
        drop(self.file);
        remove_file(&self.path)
    }
}

pub(crate) fn read_latest<T>(path: &Path, operation: &str) -> io::Result<Option<T>>
where
    T: DeserializeOwned + JournalRecord,
{
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut latest = None;
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<T>(line) else {
            break;
        };
        if record.version() != JOURNAL_VERSION {
            return Err(invalid(format!(
                "unsupported {operation} journal version {}",
                record.version()
            )));
        }
        latest = Some(record);
    }
    Ok(latest)
}

pub(crate) trait JournalRecord {
    fn version(&self) -> u32;
}

pub(crate) fn write_new(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    sync_parent(path)
}

pub(crate) fn rename(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)?;
    sync_parent(source)?;
    if source.parent() != destination.parent() {
        sync_parent(destination)?;
    }
    Ok(())
}

pub(crate) fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn remove_dir_all(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn sync_tree(root: &Path) -> io::Result<()> {
    let mut directories = vec![root.to_path_buf()];
    let mut ordered = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            }
        }
        ordered.push(directory);
    }
    for directory in ordered.into_iter().rev() {
        sync_directory(&directory)?;
    }
    sync_parent(root)
}

fn sync_parent(path: &Path) -> io::Result<()> {
    path.parent().map_or(Ok(()), sync_directory)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
