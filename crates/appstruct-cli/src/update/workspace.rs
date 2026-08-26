use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ROOT_IGNORED: &[&str] = &[
    ".git",
    ".appstruct",
    "generated",
    ".generated.appstruct-update-staging",
    ".generated.appstruct-update-backup",
    ".appstruct.lock.appstruct-update-staging",
    ".appstruct.lock.appstruct-update-backup",
];
const TRANSIENT_DIRECTORIES: &[&str] = &["target", "node_modules", "dist", ".vite"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectSnapshot {
    files: BTreeMap<PathBuf, String>,
}

pub(super) struct CandidateWorkspace {
    directory: tempfile::TempDir,
    snapshot: ProjectSnapshot,
}

impl CandidateWorkspace {
    pub(super) fn prepare(project: &Path) -> io::Result<Self> {
        let state = project.join(".appstruct");
        fs::create_dir_all(&state)?;
        let directory = tempfile::Builder::new()
            .prefix("update-workspace-")
            .tempdir_in(state)?;
        let mut files = BTreeMap::new();
        copy_directory(project, directory.path(), Path::new(""), &mut files)?;
        Ok(Self {
            directory,
            snapshot: ProjectSnapshot { files },
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.directory.path()
    }

    pub(super) fn ensure_source_unchanged(&self, project: &Path) -> io::Result<()> {
        let current = snapshot_directory(project, Path::new(""))?;
        if self.snapshot.files == current {
            Ok(())
        } else {
            Err(io::Error::other(
                "project files changed while the staged update was being verified",
            ))
        }
    }
}

fn copy_directory(
    source: &Path,
    destination: &Path,
    relative: &Path,
    files: &mut BTreeMap<PathBuf, String>,
) -> io::Result<()> {
    for entry in sorted_entries(source)? {
        let name = entry.file_name();
        let child_relative = relative.join(&name);
        if ignored(&child_relative, &name) {
            continue;
        }
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let destination_path = destination.join(&child_relative);
        if file_type.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_directory(&source_path, destination, &child_relative, files)?;
        } else if file_type.is_file() {
            let content = fs::read(&source_path)?;
            let parent = destination_path
                .parent()
                .ok_or_else(|| invalid("candidate file has no parent"))?;
            fs::create_dir_all(parent)?;
            fs::write(&destination_path, &content)?;
            fs::set_permissions(&destination_path, fs::metadata(&source_path)?.permissions())?;
            files.insert(child_relative, content_hash(&content));
        } else {
            return Err(invalid(format!(
                "unsupported project entry `{}`; update staging accepts regular files only",
                source_path.display()
            )));
        }
    }
    Ok(())
}

fn snapshot_directory(root: &Path, relative: &Path) -> io::Result<BTreeMap<PathBuf, String>> {
    let mut files = BTreeMap::new();
    collect_snapshot(root, relative, &mut files)?;
    Ok(files)
}

fn collect_snapshot(
    root: &Path,
    relative: &Path,
    files: &mut BTreeMap<PathBuf, String>,
) -> io::Result<()> {
    let directory = root.join(relative);
    for entry in sorted_entries(&directory)? {
        let name = entry.file_name();
        let child_relative = relative.join(&name);
        if ignored(&child_relative, &name) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_snapshot(root, &child_relative, files)?;
        } else if file_type.is_file() {
            let content = fs::read(entry.path())?;
            files.insert(child_relative, content_hash(&content));
        } else {
            return Err(invalid(format!(
                "unsupported project entry `{}`; update staging accepts regular files only",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

fn sorted_entries(directory: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn ignored(relative: &Path, name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    (relative.components().count() == 1 && ROOT_IGNORED.contains(&name.as_ref()))
        || TRANSIENT_DIRECTORIES.contains(&name.as_ref())
}

fn content_hash(content: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(content))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
