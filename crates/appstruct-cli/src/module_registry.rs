use appstruct_module_sdk::{
    MODULE_API_VERSION, ModuleManifest, RegistryEnvelope, RegistryPackage, module_namespace,
    validate_manifest, validate_relative_path, verify_registry_envelope,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, io, path::Path, process::ExitCode, time::Duration};

mod lifecycle;
mod verification;

const LOCK_PATH: &str = "appstruct.modules.lock";
const MAX_PACKAGE_BYTES: usize = 12 * 1024 * 1024;

#[derive(Debug, Subcommand)]
pub(crate) enum ModuleCommand {
    /// Download, verify, cache, and lock a signed registry module.
    Install {
        /// Module reference in `vendor/name@version` form.
        module: String,
        /// Registry base URL.
        #[arg(long)]
        registry: String,
        /// Base64-encoded Ed25519 public key; defaults to `APPSTRUCT_REGISTRY_PUBLIC_KEY`.
        #[arg(long)]
        public_key: Option<String>,
    },
    /// Replace an installed module with an explicitly selected signed version.
    Update {
        /// Module reference in `vendor/name@version` form.
        module: String,
        /// Override the registry recorded in the lock.
        #[arg(long)]
        registry: Option<String>,
        /// Rotate the locked Ed25519 public key while updating.
        #[arg(long)]
        public_key: Option<String>,
    },
    /// Remove a locked remote module and its unreferenced cache.
    Uninstall {
        /// Installed module name in `vendor/name` form.
        module: String,
    },
    /// Revalidate locked signatures, digests, manifests, and cached artifacts offline.
    Verify {
        /// Optional installed module name; verifies all modules when omitted.
        module: Option<String>,
    },
    /// List locked remote modules without contacting a registry.
    List,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryLock {
    #[serde(default = "lock_version")]
    lock_version: u32,
    #[serde(default)]
    modules: Vec<LockedRemoteModule>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

pub(crate) fn run(project: &Path, command: &ModuleCommand) -> ExitCode {
    let result = match command {
        ModuleCommand::Install {
            module,
            registry,
            public_key,
        } => install(project, module, registry, public_key.as_deref()),
        ModuleCommand::Update {
            module,
            registry,
            public_key,
        } => lifecycle::update(project, module, registry.as_deref(), public_key.as_deref()),
        ModuleCommand::Uninstall { module } => lifecycle::uninstall(project, module),
        ModuleCommand::Verify { module } => verification::verify(project, module.as_deref()),
        ModuleCommand::List => list(project),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => crate::report::fail(
            "AS6090",
            crate::report::ErrorCategory::Project,
            error,
            crate::report::ExitClass::Environment,
        ),
    }
}

fn install(
    project: &Path,
    reference: &str,
    registry: &str,
    public_key: Option<&str>,
) -> Result<(), String> {
    let (name, version) = parse_reference(reference)?;
    validate_registry_url(registry)?;
    let public_key = public_key
        .map(str::to_owned)
        .or_else(|| env::var("APPSTRUCT_REGISTRY_PUBLIC_KEY").ok())
        .ok_or_else(|| "--public-key or APPSTRUCT_REGISTRY_PUBLIC_KEY is required".to_owned())?;
    let url = format!(
        "{}/v1/modules/{name}/{version}",
        registry.trim_end_matches('/')
    );
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("cannot create registry client: {error}"))?
        .get(&url)
        .header("accept", "application/json")
        .send()
        .map_err(|error| format!("cannot download `{url}`: {error}"))?
        .error_for_status()
        .map_err(|error| format!("registry rejected `{reference}`: {error}"))?;
    let bytes = response
        .bytes()
        .map_err(|error| format!("cannot read registry response: {error}"))?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(format!(
            "registry response exceeds {MAX_PACKAGE_BYTES} bytes"
        ));
    }
    install_envelope(project, registry, &public_key, name, version, &bytes)
}

fn install_envelope(
    project: &Path,
    registry: &str,
    public_key: &str,
    requested_name: &str,
    requested_version: &str,
    envelope_bytes: &[u8],
) -> Result<(), String> {
    let envelope: RegistryEnvelope = serde_json::from_slice(envelope_bytes)
        .map_err(|error| format!("invalid registry envelope JSON: {error}"))?;
    let (package, _) =
        verify_registry_envelope(&envelope, public_key).map_err(|error| error.to_string())?;
    let (_manifest, artifacts) = validate_package(&package, requested_name, requested_version)?;
    let namespace = module_namespace(&package.name).map_err(str::to_owned)?;
    let digest = envelope
        .sha256
        .strip_prefix("sha256:")
        .ok_or_else(|| "registry package digest is malformed".to_owned())?;
    let relative_directory = format!("modules/.registry/{namespace}/{}/{digest}", package.version);
    let destination = project.join(&relative_directory);
    if !destination.exists() {
        install_cache(
            project,
            &destination,
            envelope_bytes,
            &package.manifest,
            &artifacts,
        )?;
    }
    let manifest_path = format!("{relative_directory}/module.toml");
    let envelope_path = format!("{relative_directory}/package.envelope.json");
    let manifest_sha256 = format!("sha256:{:x}", Sha256::digest(package.manifest.as_bytes()));
    let mut lock = read_lock(project)?;
    lock.modules.retain(|module| module.name != package.name);
    lock.modules.push(LockedRemoteModule {
        name: package.name.clone(),
        version: package.version.clone(),
        registry: registry.trim_end_matches('/').to_owned(),
        public_key: public_key.to_owned(),
        package_sha256: envelope.sha256,
        envelope_path,
        manifest_path,
        manifest_sha256,
        appstruct_version: package.appstruct_version,
        module_api_version: package.module_api_version,
    });
    lock.modules
        .sort_by(|left, right| left.name.cmp(&right.name));
    write_lock(project, &lock).map_err(|error| format!("cannot write module lock: {error}"))?;
    println!(
        "Installed signed module {}@{}",
        package.name, package.version
    );
    Ok(())
}

fn validate_package(
    package: &RegistryPackage,
    requested_name: &str,
    requested_version: &str,
) -> Result<(ModuleManifest, BTreeMap<String, Vec<u8>>), String> {
    if package.name != requested_name || package.version != requested_version {
        return Err("signed package identity does not match the requested module".to_owned());
    }
    if package.appstruct_version != env!("CARGO_PKG_VERSION") {
        return Err(format!(
            "module requires AppStruct {}; current version is {}",
            package.appstruct_version,
            env!("CARGO_PKG_VERSION")
        ));
    }
    if package.module_api_version != MODULE_API_VERSION {
        return Err(format!(
            "module API {} is incompatible with supported version {MODULE_API_VERSION}",
            package.module_api_version
        ));
    }
    let mut manifest: ModuleManifest = toml::from_str(&package.manifest)
        .map_err(|error| format!("invalid signed module manifest: {error}"))?;
    validate_manifest(&mut manifest).map_err(|error| error.to_string())?;
    if manifest.name != package.name
        || manifest.version != package.version
        || manifest.api_version != package.module_api_version
    {
        return Err(
            "signed manifest identity or API version does not match the package".to_owned(),
        );
    }
    let mut artifacts = BTreeMap::new();
    let mut total = 0_usize;
    for artifact in &package.artifacts {
        validate_relative_path(&artifact.source).map_err(str::to_owned)?;
        let content = STANDARD
            .decode(&artifact.content)
            .map_err(|error| format!("invalid artifact base64: {error}"))?;
        total = total
            .checked_add(content.len())
            .ok_or_else(|| "artifact size overflow".to_owned())?;
        let digest = format!("sha256:{:x}", Sha256::digest(&content));
        if content.len() > 1024 * 1024
            || total > 8 * 1024 * 1024
            || digest != artifact.sha256
            || artifact.byte_len != content.len() as u64
        {
            return Err(format!(
                "artifact `{}` failed size or digest validation",
                artifact.source
            ));
        }
        if artifacts.insert(artifact.source.clone(), content).is_some() {
            return Err(format!(
                "artifact source `{}` is duplicated",
                artifact.source
            ));
        }
    }
    let declared = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.source.as_str())
        .collect::<Vec<_>>();
    if declared.len() != artifacts.len()
        || declared
            .iter()
            .any(|source| !artifacts.contains_key(*source))
    {
        return Err("signed artifact set does not match the manifest".to_owned());
    }
    Ok((manifest, artifacts))
}

fn install_cache(
    project: &Path,
    destination: &Path,
    envelope: &[u8],
    manifest: &str,
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let registry_root = project.join("modules/.registry");
    fs::create_dir_all(&registry_root).map_err(|error| error.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&registry_root)
        .map_err(|error| error.to_string())?;
    fs::write(staging.path().join("module.toml"), manifest).map_err(|error| error.to_string())?;
    fs::write(staging.path().join("package.envelope.json"), envelope)
        .map_err(|error| error.to_string())?;
    for (source, content) in artifacts {
        let path = staging.path().join(source);
        fs::create_dir_all(path.parent().expect("artifact has a parent"))
            .map_err(|error| error.to_string())?;
        fs::write(path, content).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(destination.parent().expect("cache has a parent"))
        .map_err(|error| error.to_string())?;
    let path = staging.keep();
    fs::rename(path, destination).map_err(|error| error.to_string())
}

fn list(project: &Path) -> Result<(), String> {
    for module in read_lock(project)?.modules {
        println!(
            "{}@{} {}",
            module.name, module.version, module.package_sha256
        );
    }
    Ok(())
}

fn read_lock(project: &Path) -> Result<RegistryLock, String> {
    match fs::read_to_string(project.join(LOCK_PATH)) {
        Ok(source) => {
            let lock: RegistryLock = toml::from_str(&source)
                .map_err(|error| format!("invalid `{LOCK_PATH}`: {error}"))?;
            if lock.lock_version != lock_version() {
                return Err(format!("unsupported `{LOCK_PATH}` version"));
            }
            Ok(lock)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(RegistryLock {
            lock_version: lock_version(),
            modules: Vec::new(),
        }),
        Err(error) => Err(format!("cannot read `{LOCK_PATH}`: {error}")),
    }
}

fn write_lock(project: &Path, lock: &RegistryLock) -> io::Result<()> {
    let source = toml::to_string_pretty(lock).map_err(io::Error::other)?;
    let mut temporary = tempfile::NamedTempFile::new_in(project)?;
    io::Write::write_all(&mut temporary, source.as_bytes())?;
    temporary
        .persist(project.join(LOCK_PATH))
        .map_err(|error| error.error)?;
    Ok(())
}

fn parse_reference(reference: &str) -> Result<(&str, &str), String> {
    reference
        .rsplit_once('@')
        .filter(|(name, version)| !name.is_empty() && !version.is_empty())
        .ok_or_else(|| "module reference must use `vendor/name@version`".to_owned())
}

fn validate_registry_url(registry: &str) -> Result<(), String> {
    if registry.starts_with("https://")
        || registry.starts_with("http://localhost:")
        || registry.starts_with("http://127.0.0.1:")
    {
        Ok(())
    } else {
        Err("registry URL must use HTTPS (HTTP is allowed only for localhost)".to_owned())
    }
}

const fn lock_version() -> u32 {
    1
}

#[cfg(test)]
mod tests;
