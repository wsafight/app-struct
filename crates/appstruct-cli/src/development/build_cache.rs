use crate::cache::{CacheKey, command_identity};
use crate::environment::{CacheEnvironment, ProjectEnvironment};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
struct CacheMarker;

pub(super) fn backend_current(
    project: &Path,
    environment: &ProjectEnvironment,
) -> io::Result<bool> {
    let binary = project
        .join(".appstruct/cache/backend-target/debug")
        .join(crate::build::backend_binary_name(project)?);
    current(
        &project.join(".appstruct/cache/backend-build.json"),
        &backend_key(project, environment)?,
        binary.is_file(),
    )
}

pub(super) fn record_backend(project: &Path, environment: &ProjectEnvironment) -> io::Result<()> {
    record(
        &project.join(".appstruct/cache/backend-build.json"),
        backend_key(project, environment)?,
    )
}

pub(super) fn web_dependencies_current(
    project: &Path,
    environment: &ProjectEnvironment,
) -> io::Result<bool> {
    let web = project.join("generated/web");
    current(
        &project.join(".appstruct/cache/web-install.json"),
        &web_key(project, environment)?,
        web.join("node_modules/.pnpm").is_dir(),
    )
}

pub(super) fn record_web_dependencies(
    project: &Path,
    environment: &ProjectEnvironment,
) -> io::Result<()> {
    record(
        &project.join(".appstruct/cache/web-install.json"),
        web_key(project, environment)?,
    )
}

fn backend_key(project: &Path, environment: &ProjectEnvironment) -> io::Result<CacheKey> {
    let mut files = Vec::new();
    for directory in ["generated/backend", "generated/server", "app/backend"] {
        collect_files(&project.join(directory), &mut files)?;
    }
    collect_files(&project.join(".cargo"), &mut files)?;
    for relative in ["rust-toolchain.toml", "rust-toolchain"] {
        let path = project.join(relative);
        if path.is_file() {
            files.push(path);
        }
    }
    let mut cargo = Command::new("cargo");
    cargo.current_dir(project).arg("-Vv");
    environment.apply(&mut cargo);
    let mut rustc = Command::new("rustc");
    rustc.current_dir(project).arg("-vV");
    environment.apply(&mut rustc);
    Ok(
        CacheKey::new("backend-debug-build", files_fingerprint(&files)?)
            .with_tool("cargo", command_identity(&mut cargo, "cargo")?)
            .with_tool("rustc", command_identity(&mut rustc, "rustc")?)
            .with_environment(environment.cache_fingerprint(CacheEnvironment::Rust)),
    )
}

fn web_key(project: &Path, environment: &ProjectEnvironment) -> io::Result<CacheKey> {
    let web = project.join("generated/web");
    let mut files = vec![web.join("package.json"), web.join("pnpm-lock.yaml")];
    for relative in [".npmrc", "pnpm-workspace.yaml"] {
        let path = project.join(relative);
        if path.is_file() {
            files.push(path);
        }
    }
    let mut pnpm = Command::new("pnpm");
    pnpm.current_dir(project).arg("--version");
    environment.apply(&mut pnpm);
    Ok(
        CacheKey::new("web-dependency-install", files_fingerprint(&files)?)
            .with_tool("pnpm", command_identity(&mut pnpm, "pnpm")?)
            .with_environment(environment.cache_fingerprint(CacheEnvironment::Node)),
    )
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

fn current(path: &Path, key: &CacheKey, output_exists: bool) -> io::Result<bool> {
    if !output_exists {
        return Ok(false);
    }
    crate::cache::load::<CacheMarker>(path, key).map(|state| state.is_some())
}

fn record(path: &Path, key: CacheKey) -> io::Result<()> {
    crate::cache::store(path, key, CacheMarker)
}

#[cfg(test)]
mod tests {
    use super::{backend_current, record_backend};
    use crate::environment::ProjectEnvironment;
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
        let environment = ProjectEnvironment::default();

        assert!(!backend_current(project.path(), &environment).unwrap());
        record_backend(project.path(), &environment).unwrap();
        assert!(backend_current(project.path(), &environment).unwrap());
        fs::write(
            project.path().join("generated/backend/src/lib.rs"),
            "pub fn value() -> u32 { 2 }\n",
        )
        .unwrap();
        assert!(!backend_current(project.path(), &environment).unwrap());
    }
}
