use crate::loading::synthetic_span;
use crate::module::{LoadedModule, load_remote_module, read_isolated_file};
use appstruct_ir::Diagnostic;
use appstruct_module_sdk::{
    MODULE_API_VERSION, RegistryEnvelope, RegistryPackage, verify_registry_envelope,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const LOCK_PATH: &str = "appstruct.modules.lock";
const MAX_ENVELOPE_BYTES: usize = 12 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryLock {
    lock_version: u32,
    #[serde(default)]
    modules: Vec<LockedRemoteModule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedRemoteModule {
    name: String,
    version: String,
    registry: String,
    public_key: String,
    package_sha256: String,
    envelope_path: String,
    manifest_path: String,
    manifest_sha256: String,
    appstruct_version: String,
    module_api_version: u32,
}

pub(crate) fn load(project: &Path) -> (Vec<LoadedModule>, Vec<Diagnostic>) {
    let path = project.join(LOCK_PATH);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (Vec::new(), Vec::new());
        }
        Err(error) => {
            return (
                Vec::new(),
                vec![diagnostic(format!(
                    "cannot read remote module lock: {error}"
                ))],
            );
        }
    };
    let lock = match toml::from_str::<RegistryLock>(&source) {
        Ok(lock) => lock,
        Err(error) => {
            return (
                Vec::new(),
                vec![diagnostic(format!("invalid remote module lock: {error}"))],
            );
        }
    };
    if lock.lock_version != 1 {
        return (
            Vec::new(),
            vec![diagnostic("unsupported remote module lock version")],
        );
    }
    let mut modules = Vec::with_capacity(lock.modules.len());
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    for locked in lock.modules {
        if !names.insert(locked.name.clone()) {
            diagnostics.push(diagnostic(format!(
                "remote module `{}` is locked more than once",
                locked.name
            )));
            continue;
        }
        match load_one(project, &locked) {
            Ok(module) => modules.push(module),
            Err(error) => diagnostics.push(diagnostic(format!(
                "remote module `{}`: {error}",
                locked.name
            ))),
        }
    }
    (modules, diagnostics)
}

fn load_one(project: &Path, locked: &LockedRemoteModule) -> Result<LoadedModule, String> {
    if locked.registry.is_empty() {
        return Err("registry URL is empty".to_owned());
    }
    if locked.appstruct_version != env!("CARGO_PKG_VERSION") {
        return Err("AppStruct compatibility does not match; reinstall the module".to_owned());
    }
    if locked.module_api_version != MODULE_API_VERSION {
        return Err("Module API compatibility does not match; reinstall the module".to_owned());
    }
    for path in [&locked.envelope_path, &locked.manifest_path] {
        if !path.starts_with("modules/.registry/") {
            return Err(format!(
                "locked path `{path}` is outside `modules/.registry/`"
            ));
        }
    }
    let envelope = read_isolated_file(project, &locked.envelope_path, MAX_ENVELOPE_BYTES)?;
    let envelope: RegistryEnvelope = serde_json::from_slice(&envelope)
        .map_err(|error| format!("invalid cached envelope: {error}"))?;
    if envelope.sha256 != locked.package_sha256 {
        return Err("cached envelope digest differs from the lock".to_owned());
    }
    let (package, _) = verify_registry_envelope(&envelope, &locked.public_key)
        .map_err(|error| error.to_string())?;
    validate_package(&package, locked)?;
    let module =
        load_remote_module(project, &locked.manifest_path).map_err(|error| error.message)?;
    if module.content_sha256 != locked.manifest_sha256
        || module.manifest.name != locked.name
        || module.manifest.version != locked.version
    {
        return Err("cached manifest provenance differs from the lock".to_owned());
    }
    validate_artifacts(&package, &module)?;
    Ok(module)
}

fn validate_package(package: &RegistryPackage, locked: &LockedRemoteModule) -> Result<(), String> {
    if package.name != locked.name || package.version != locked.version {
        return Err("signed package identity differs from the lock".to_owned());
    }
    if package.appstruct_version != locked.appstruct_version
        || package.module_api_version != locked.module_api_version
    {
        return Err("signed package compatibility differs from the lock".to_owned());
    }
    let digest = format!("sha256:{:x}", Sha256::digest(package.manifest.as_bytes()));
    if digest != locked.manifest_sha256 {
        return Err("signed manifest digest differs from the lock".to_owned());
    }
    Ok(())
}

fn validate_artifacts(package: &RegistryPackage, module: &LoadedModule) -> Result<(), String> {
    let package_artifacts = package
        .artifacts
        .iter()
        .map(|artifact| (artifact.source.as_str(), artifact))
        .collect::<BTreeMap<_, _>>();
    if package_artifacts.len() != module.artifacts.len() {
        return Err("signed artifact set differs from the cached manifest".to_owned());
    }
    for (manifest_artifact, loaded) in module.manifest.artifacts.iter().zip(&module.artifacts) {
        let artifact = package_artifacts
            .get(manifest_artifact.source.as_str())
            .ok_or_else(|| format!("signed artifact `{}` is missing", manifest_artifact.source))?;
        let content = STANDARD
            .decode(&artifact.content)
            .map_err(|error| format!("artifact base64 is invalid: {error}"))?;
        let digest = format!("sha256:{:x}", Sha256::digest(&content));
        if digest != artifact.sha256
            || artifact.sha256 != loaded.sha256
            || artifact.byte_len != loaded.byte_len
            || content != loaded.content.as_bytes()
        {
            return Err(format!(
                "artifact `{}` failed provenance validation",
                artifact.source
            ));
        }
    }
    Ok(())
}

fn diagnostic(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("AS3090", message, synthetic_span(LOCK_PATH))
        .with_help("run `appstruct module install` to refresh the signed module cache and lock")
}
