use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const STATE_VERSION: u32 = 1;

#[derive(Debug, Deserialize, Serialize)]
struct BuildState {
    version: u32,
    fingerprint: String,
}

pub(super) fn backend_current(project: &Path) -> io::Result<bool> {
    let binary = project
        .join(".appstruct/cache/backend-target/debug")
        .join(crate::build::backend_binary_name(project)?);
    current(
        &project.join(".appstruct/cache/backend-build.json"),
        &backend_fingerprint(project)?,
        binary.is_file(),
    )
}

pub(super) fn record_backend(project: &Path) -> io::Result<()> {
    record(
        &project.join(".appstruct/cache/backend-build.json"),
        backend_fingerprint(project)?,
    )
}

pub(super) fn web_dependencies_current(project: &Path) -> io::Result<bool> {
    let web = project.join("generated/web");
    current(
        &project.join(".appstruct/cache/web-install.json"),
        &files_fingerprint(&[web.join("package.json"), web.join("pnpm-lock.yaml")])?,
        web.join("node_modules/.pnpm").is_dir(),
    )
}

pub(super) fn record_web_dependencies(project: &Path) -> io::Result<()> {
    let web = project.join("generated/web");
    record(
        &project.join(".appstruct/cache/web-install.json"),
        files_fingerprint(&[web.join("package.json"), web.join("pnpm-lock.yaml")])?,
    )
}

fn backend_fingerprint(project: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    for directory in ["generated/backend", "generated/server", "app/backend"] {
        collect_files(&project.join(directory), &mut files)?;
    }
    files_fingerprint(&files)
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
            if entry.file_name() != "target" {
                collect_files(&path, files)?;
            }
        } else if file_type.is_file() {
            files.push(path);
        } else if file_type.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "backend build cache does not accept symbolic link `{}`",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn files_fingerprint(files: &[PathBuf]) -> io::Result<String> {
    let mut files = files.to_vec();
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        hasher.update(path.to_string_lossy().as_bytes());
        let mut file = File::open(path)?;
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn current(path: &Path, fingerprint: &str, output_exists: bool) -> io::Result<bool> {
    if !output_exists {
        return Ok(false);
    }
    let source = match fs::read(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let state: BuildState = match serde_json::from_slice(&source) {
        Ok(state) => state,
        Err(_) => return Ok(false),
    };
    Ok(state.version == STATE_VERSION && state.fingerprint == fingerprint)
}

fn record(path: &Path, fingerprint: String) -> io::Result<()> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("build cache state has no parent"))?,
    )?;
    let state = BuildState {
        version: STATE_VERSION,
        fingerprint,
    };
    let mut source = serde_json::to_vec_pretty(&state).map_err(io::Error::other)?;
    source.push(b'\n');
    fs::write(path, source)
}

#[cfg(test)]
mod tests {
    use super::{backend_current, record_backend};
    use std::fs;

    #[test]
    fn backend_cache_requires_matching_inputs_and_binary() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("generated/backend/src")).unwrap();
        fs::create_dir_all(project.path().join(".appstruct/cache/backend-target/debug")).unwrap();
        fs::write(
            project.path().join("generated/backend/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        )
        .unwrap();
        fs::write(
            project
                .path()
                .join(".appstruct/cache/backend-target/debug/appstruct-generated-backend"),
            "binary",
        )
        .unwrap();

        assert!(!backend_current(project.path()).unwrap());
        record_backend(project.path()).unwrap();
        assert!(backend_current(project.path()).unwrap());
        fs::write(
            project.path().join("generated/backend/src/lib.rs"),
            "pub fn value() -> u32 { 2 }\n",
        )
        .unwrap();
        assert!(!backend_current(project.path()).unwrap());
    }
}
