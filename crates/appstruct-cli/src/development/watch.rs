use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ProjectChanges {
    pub(super) specification: bool,
    pub(super) backend: bool,
    pub(super) web: bool,
}

impl ProjectChanges {
    fn merge(&mut self, other: Self) {
        self.specification |= other.specification;
        self.backend |= other.backend;
        self.web |= other.web;
    }

    fn is_empty(self) -> bool {
        !self.specification && !self.backend && !self.web
    }
}

pub(super) struct ProjectWatcher {
    project: PathBuf,
    mode: WatchMode,
}

enum WatchMode {
    Native(NativeWatcher),
    Polling(SourceFingerprints),
}

struct NativeWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
}

impl ProjectWatcher {
    pub(super) fn start(project: &Path) -> io::Result<Self> {
        let project = fs::canonicalize(project)?;
        let mode = match NativeWatcher::start(&project) {
            Ok(watcher) => WatchMode::Native(watcher),
            Err(error) => {
                eprintln!(
                    "[appstruct] native file watching unavailable ({error}); using polling fallback"
                );
                WatchMode::Polling(SourceFingerprints::read(&project)?)
            }
        };
        Ok(Self { project, mode })
    }

    pub(super) fn changes(&mut self, timeout: Duration) -> io::Result<Option<ProjectChanges>> {
        if let WatchMode::Native(watcher) = &self.mode {
            match watcher.changes(&self.project, timeout) {
                Ok(changes) => return Ok(changes),
                Err(error) => {
                    eprintln!(
                        "[appstruct] native file watching stopped ({error}); using polling fallback"
                    );
                    self.mode = WatchMode::Polling(SourceFingerprints::read(&self.project)?);
                    return Ok(None);
                }
            }
        }
        let WatchMode::Polling(fingerprints) = &mut self.mode else {
            unreachable!();
        };
        thread::sleep(timeout);
        let next = SourceFingerprints::read(&self.project)?;
        let changes = fingerprints.changes(&next);
        *fingerprints = next;
        Ok((!changes.is_empty()).then_some(changes))
    }

    pub(super) fn refresh(&mut self) -> io::Result<()> {
        if matches!(self.mode, WatchMode::Native(_)) {
            self.mode = match NativeWatcher::start(&self.project) {
                Ok(watcher) => WatchMode::Native(watcher),
                Err(error) => {
                    eprintln!(
                        "[appstruct] native file watching stopped ({error}); using polling fallback"
                    );
                    WatchMode::Polling(SourceFingerprints::read(&self.project)?)
                }
            };
        }
        Ok(())
    }
}

impl NativeWatcher {
    fn start(project: &Path) -> io::Result<Self> {
        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(notify_error)?;
        watcher
            .watch(project, RecursiveMode::NonRecursive)
            .map_err(notify_error)?;
        for relative in ["spec", "modules", "app/backend", "app/web", ".cargo"] {
            let directory = project.join(relative);
            if directory.is_dir() {
                watcher
                    .watch(&directory, RecursiveMode::Recursive)
                    .map_err(notify_error)?;
            }
        }
        Ok(Self {
            _watcher: watcher,
            events,
        })
    }

    fn changes(&self, project: &Path, timeout: Duration) -> io::Result<Option<ProjectChanges>> {
        let deadline = Instant::now() + timeout;
        let mut changes = ProjectChanges::default();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self.events.recv_timeout(remaining) {
                Ok(event) => {
                    changes.merge(changes_from_event(project, event.map_err(notify_error)?));
                    if !changes.is_empty() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("native file watcher disconnected"));
                }
            }
        }
        loop {
            match self.events.recv_timeout(DEBOUNCE) {
                Ok(event) => {
                    changes.merge(changes_from_event(project, event.map_err(notify_error)?));
                }
                Err(RecvTimeoutError::Timeout) => return Ok(Some(changes)),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("native file watcher disconnected"));
                }
            }
        }
    }
}

fn changes_from_event(project: &Path, event: Event) -> ProjectChanges {
    if matches!(event.kind, EventKind::Access(_)) {
        return ProjectChanges::default();
    }
    let mut changes = ProjectChanges::default();
    for path in event.paths {
        changes.merge(classify_path(project, &path));
    }
    changes
}

fn classify_path(project: &Path, path: &Path) -> ProjectChanges {
    let Ok(relative) = path.strip_prefix(project) else {
        return ProjectChanges::default();
    };
    if relative.starts_with("spec") || relative.starts_with("modules") {
        return ProjectChanges {
            specification: true,
            backend: true,
            web: true,
        };
    }
    if relative.starts_with("app/backend") || relative.starts_with(".cargo") {
        return ProjectChanges {
            backend: true,
            ..ProjectChanges::default()
        };
    }
    if relative.starts_with("app/web") {
        return ProjectChanges {
            web: true,
            ..ProjectChanges::default()
        };
    }
    if matches!(
        relative.to_str(),
        Some(
            "appstruct.yaml"
                | "appstruct.lock"
                | "appstruct.modules.lock"
                | ".env"
                | ".npmrc"
                | "pnpm-workspace.yaml"
        )
    ) {
        return ProjectChanges {
            specification: true,
            backend: true,
            web: true,
        };
    }
    if matches!(
        relative.to_str(),
        Some("rust-toolchain.toml" | "rust-toolchain")
    ) {
        return ProjectChanges {
            backend: true,
            ..ProjectChanges::default()
        };
    }
    ProjectChanges::default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceFingerprints {
    specification: [u8; 32],
    backend: [u8; 32],
    web: [u8; 32],
}

impl SourceFingerprints {
    fn read(project: &Path) -> io::Result<Self> {
        Ok(Self {
            specification: fingerprint(
                project,
                &[
                    "appstruct.yaml",
                    "appstruct.lock",
                    "appstruct.modules.lock",
                    ".env",
                    ".npmrc",
                    "pnpm-workspace.yaml",
                ],
                &["spec", "modules"],
            )?,
            backend: fingerprint(
                project,
                &["rust-toolchain.toml", "rust-toolchain"],
                &["app/backend", ".cargo"],
            )?,
            web: fingerprint(project, &[], &["app/web"])?,
        })
    }

    fn changes(&self, next: &Self) -> ProjectChanges {
        ProjectChanges {
            specification: self.specification != next.specification,
            backend: self.backend != next.backend,
            web: self.web != next.web,
        }
    }
}

fn fingerprint(project: &Path, files: &[&str], directories: &[&str]) -> io::Result<[u8; 32]> {
    let mut paths = files
        .iter()
        .map(|path| project.join(path))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    for directory in directories {
        collect_files(&project.join(directory), &mut paths)?;
    }
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        let relative = path.strip_prefix(project).map_err(io::Error::other)?;
        hash.update(relative.to_string_lossy().as_bytes());
        hash.update([0]);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            hash.update(b"symlink\0");
            hash.update(fs::read_link(path)?.as_os_str().as_encoded_bytes());
        } else {
            hash.update(fs::read(path)?);
        }
        hash.update([0]);
    }
    Ok(hash.finalize().into())
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(&path, files)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            files.push(path);
        }
    }
    Ok(())
}

fn notify_error(error: notify::Error) -> io::Error {
    io::Error::other(error)
}

#[cfg(test)]
mod tests;
