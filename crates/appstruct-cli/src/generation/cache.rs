use crate::cache::{CacheKey, command_identity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

const STATE_PATH: &str = ".appstruct/cache/generation-state.json";
const MANIFEST_PATH: &str = ".appstruct-manifest.json";

#[derive(Debug, Deserialize, Serialize)]
struct GenerationState {
    manifest_sha256: String,
    app_name: String,
    artifact_count: usize,
}

pub(super) struct CacheHit {
    pub app_name: String,
    pub artifact_count: usize,
}

pub(super) fn load_hit(project: &Path, generated: &Path) -> io::Result<Option<CacheHit>> {
    let key = generation_key(project)?;
    let Some(state) = crate::cache::load::<GenerationState>(&project.join(STATE_PATH), &key)?
    else {
        return Ok(None);
    };
    if !generated.is_dir() {
        return Ok(None);
    }
    super::ownership::validate_owned_tree(generated)?;
    let manifest = generated.join(MANIFEST_PATH);
    if state.manifest_sha256 != file_fingerprint(&manifest)? {
        return Ok(None);
    }
    Ok(Some(CacheHit {
        app_name: state.app_name,
        artifact_count: state.artifact_count,
    }))
}

pub(super) fn record(
    project: &Path,
    generated: &Path,
    app_name: &str,
    artifact_count: usize,
) -> io::Result<()> {
    let state = GenerationState {
        manifest_sha256: file_fingerprint(&generated.join(MANIFEST_PATH))?,
        app_name: app_name.to_owned(),
        artifact_count,
    };
    crate::cache::store(&project.join(STATE_PATH), generation_key(project)?, state)
}

fn generation_key(project: &Path) -> io::Result<CacheKey> {
    let pnpm = command_identity(Command::new("pnpm").arg("--version"), "pnpm")?;
    Ok(CacheKey::new("generation", input_fingerprint(project)?)
        .with_tool("appstruct", executable_fingerprint()?)
        .with_tool("pnpm", pnpm))
}

fn input_fingerprint(project: &Path) -> io::Result<String> {
    let mut paths = ["appstruct.yaml", "appstruct.lock"]
        .into_iter()
        .map(|path| project.join(path))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    collect_directory(&project.join("spec"), &mut paths)?;
    collect_directory(&project.join("modules"), &mut paths)?;
    paths.sort();
    let mut hasher = Sha256::new();
    for path in paths {
        let relative = path.strip_prefix(project).map_err(io::Error::other)?;
        hasher.update(relative.to_string_lossy().as_bytes());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            hasher.update(b"symlink\0");
            hasher.update(fs::read_link(&path)?.as_os_str().as_encoded_bytes());
        } else {
            hash_file(&path, &mut hasher)?;
        }
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn collect_directory(directory: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
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
            collect_directory(&path, paths)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            paths.push(path);
        }
    }
    Ok(())
}

fn executable_fingerprint() -> io::Result<String> {
    file_fingerprint(&std::env::current_exe()?)
}

fn file_fingerprint(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    hash_file(path, &mut hasher)?;
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..read]);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::input_fingerprint;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn module_cache_fingerprint_does_not_follow_directory_symlinks() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("modules")).unwrap();
        symlink(project.path(), project.path().join("modules/loop")).unwrap();

        assert!(input_fingerprint(project.path()).is_ok());
    }
}
