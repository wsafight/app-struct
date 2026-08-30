use super::verification::{cache_directory, checked_cache_directory};
use super::{install, read_lock, write_lock};
use appstruct_module_sdk::module_namespace;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn update(
    project: &Path,
    reference: &str,
    registry: Option<&str>,
    public_key: Option<&str>,
) -> Result<(), String> {
    let (name, _) = super::parse_reference(reference)?;
    let lock = read_lock(project)?;
    let installed = lock
        .modules
        .iter()
        .find(|module| module.name == name)
        .ok_or_else(|| format!("module `{name}` is not installed"))?;
    let registry = registry.unwrap_or(&installed.registry).to_owned();
    let public_key = public_key.unwrap_or(&installed.public_key).to_owned();
    let old_directory = cache_directory(installed)?;
    install(project, reference, &registry, Some(&public_key))?;
    let lock = read_lock(project)?;
    let current = lock
        .modules
        .iter()
        .find(|module| module.name == name)
        .ok_or_else(|| format!("module `{name}` disappeared from the lock after update"))?;
    let new_directory = cache_directory(current)?;
    if old_directory != new_directory
        && !cache_is_referenced(&lock, &old_directory)?
        && let Err(error) = remove_cache(project, &old_directory)
    {
        eprintln!("warning: updated module but could not remove old cache: {error}");
    }
    Ok(())
}

pub(super) fn uninstall(project: &Path, name: &str) -> Result<(), String> {
    module_namespace(name).map_err(str::to_owned)?;
    let mut lock = read_lock(project)?;
    let index = lock
        .modules
        .iter()
        .position(|module| module.name == name)
        .ok_or_else(|| format!("module `{name}` is not installed"))?;
    let removed = lock.modules.remove(index);
    let directory = cache_directory(&removed)?;
    let cache = if cache_is_referenced(&lock, &directory)? {
        None
    } else {
        removable_cache_path(project, &directory)?
    };
    write_lock(project, &lock).map_err(|error| format!("cannot write module lock: {error}"))?;
    if let Some(cache) = cache {
        fs::remove_dir_all(&cache)
            .map_err(|error| format!("cannot remove module cache `{directory}`: {error}"))?;
    }
    println!(
        "Uninstalled signed module {}@{}",
        removed.name, removed.version
    );
    Ok(())
}

fn cache_is_referenced(lock: &super::RegistryLock, directory: &str) -> Result<bool, String> {
    lock.modules
        .iter()
        .map(cache_directory)
        .collect::<Result<Vec<_>, _>>()
        .map(|directories| directories.iter().any(|candidate| candidate == directory))
}

fn remove_cache(project: &Path, relative: &str) -> Result<(), String> {
    if let Some(path) = removable_cache_path(project, relative)? {
        fs::remove_dir_all(&path)
            .map_err(|error| format!("cannot remove module cache `{relative}`: {error}"))?;
    }
    Ok(())
}

fn removable_cache_path(project: &Path, relative: &str) -> Result<Option<PathBuf>, String> {
    let path = checked_cache_directory(project, relative)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(Some(path)),
        Ok(_) => Err(format!("module cache `{relative}` is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot inspect module cache `{relative}`: {error}")),
    }
}
