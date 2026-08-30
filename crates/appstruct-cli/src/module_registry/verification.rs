use super::{
    LOCK_PATH, LockedRemoteModule, MAX_PACKAGE_BYTES, RegistryLock, read_lock, validate_package,
};
use appstruct_module_sdk::{module_namespace, validate_relative_path, verify_registry_envelope};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn verify(project: &Path, requested: Option<&str>) -> Result<(), String> {
    if let Some(name) = requested {
        module_namespace(name).map_err(str::to_owned)?;
    }
    let lock = read_lock(project)?;
    let selected = select_modules(&lock, requested)?;
    let mut names = BTreeSet::new();
    for module in selected {
        if !names.insert(module.name.as_str()) {
            return Err(format!("module `{}` is locked more than once", module.name));
        }
        verify_one(project, module)?;
        println!("Verified signed module {}@{}", module.name, module.version);
    }
    Ok(())
}

fn select_modules<'lock>(
    lock: &'lock RegistryLock,
    requested: Option<&str>,
) -> Result<Vec<&'lock LockedRemoteModule>, String> {
    if let Some(name) = requested {
        let module = lock
            .modules
            .iter()
            .find(|module| module.name == name)
            .ok_or_else(|| format!("module `{name}` is not installed"))?;
        return Ok(vec![module]);
    }
    Ok(lock.modules.iter().collect())
}

fn verify_one(project: &Path, locked: &LockedRemoteModule) -> Result<(), String> {
    let directory = cache_directory(locked)?;
    let envelope_bytes = read_cache_file(project, &locked.envelope_path, MAX_PACKAGE_BYTES)?;
    let envelope: appstruct_module_sdk::RegistryEnvelope = serde_json::from_slice(&envelope_bytes)
        .map_err(|error| {
            format!(
                "module `{}` has invalid cached envelope JSON: {error}",
                locked.name
            )
        })?;
    if envelope.sha256 != locked.package_sha256 {
        return Err(format!(
            "module `{}` cached package digest differs from the lock",
            locked.name
        ));
    }
    let (package, _) = verify_registry_envelope(&envelope, &locked.public_key)
        .map_err(|error| format!("module `{}`: {error}", locked.name))?;
    if package.appstruct_version != locked.appstruct_version
        || package.module_api_version != locked.module_api_version
    {
        return Err(format!(
            "module `{}` compatibility metadata differs from the lock",
            locked.name
        ));
    }
    let (_, artifacts) = validate_package(&package, &locked.name, &locked.version)?;
    let manifest = read_cache_file(project, &locked.manifest_path, 1024 * 1024)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest));
    if manifest != package.manifest.as_bytes() || manifest_digest != locked.manifest_sha256 {
        return Err(format!(
            "module `{}` cached manifest differs from the signed package",
            locked.name
        ));
    }
    for (source, expected) in artifacts {
        let relative = format!("{directory}/{source}");
        let actual = read_cache_file(project, &relative, 1024 * 1024)?;
        if actual != expected {
            return Err(format!(
                "module `{}` cached artifact `{source}` differs from the signed package",
                locked.name
            ));
        }
    }
    Ok(())
}

pub(super) fn cache_directory(locked: &LockedRemoteModule) -> Result<String, String> {
    let namespace = module_namespace(&locked.name).map_err(str::to_owned)?;
    let digest = locked
        .package_sha256
        .strip_prefix("sha256:")
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| format!("module `{}` has an invalid package digest", locked.name))?;
    let expected = format!("modules/.registry/{namespace}/{}/{digest}", locked.version);
    if locked.envelope_path != format!("{expected}/package.envelope.json")
        || locked.manifest_path != format!("{expected}/module.toml")
    {
        return Err(format!(
            "module `{}` has non-canonical cache paths in `{LOCK_PATH}`",
            locked.name
        ));
    }
    Ok(expected)
}

pub(super) fn checked_cache_directory(project: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_cache_path(relative)?;
    let mut current = project.to_path_buf();
    for component in Path::new(relative).components() {
        let Component::Normal(component) = component else {
            return Err(format!("invalid module cache path `{relative}`"));
        };
        current.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(format!("module cache path `{relative}` contains a symlink"));
        }
    }
    Ok(current)
}

fn read_cache_file(project: &Path, relative: &str, maximum: usize) -> Result<Vec<u8>, String> {
    let path = checked_cache_directory(project, relative)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("cannot inspect module cache `{relative}`: {error}"))?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err(format!(
            "module cache file `{relative}` has an invalid type or size"
        ));
    }
    fs::read(path).map_err(|error| format!("cannot read module cache `{relative}`: {error}"))
}

fn validate_cache_path(relative: &str) -> Result<(), String> {
    validate_relative_path(relative).map_err(str::to_owned)?;
    if relative.starts_with("modules/.registry/") {
        Ok(())
    } else {
        Err(format!(
            "module cache path `{relative}` is outside `modules/.registry/`"
        ))
    }
}
