use crate::surface::Located;
use appstruct_ir::{
    AuditIr, AuthIr, Diagnostic, FileIr, JobsIr, MailIr, ModuleArtifactIr, ModuleOrigin,
    ResolvedModule, TenantIr,
};
use appstruct_module_sdk::{
    ModuleGraphError, ModuleManifest, resolve_modules, validate_manifest, validate_relative_path,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

const AUTH_IDENTITY: &str = "auth.identity";
const MAIL_DELIVERY: &str = "mail.delivery";
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
const MAX_MODULE_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct LoadedModule {
    manifest: ModuleManifest,
    artifacts: Vec<ModuleArtifactIr>,
}

pub(crate) fn load_local_modules(
    project_root: &Path,
    declarations: &[Located<String>],
) -> (Vec<LoadedModule>, Vec<Diagnostic>) {
    let mut modules = Vec::with_capacity(declarations.len());
    let mut diagnostics = Vec::new();
    for declaration in declarations {
        match load_local_module(project_root, declaration) {
            Ok(module) => modules.push(module),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    (modules, diagnostics)
}

fn load_local_module(
    project_root: &Path,
    declaration: &Located<String>,
) -> Result<LoadedModule, Diagnostic> {
    validate_relative_path(&declaration.value)
        .map_err(|reason| module_path_error(declaration, reason))?;
    if Path::new(&declaration.value).components().next()
        != Some(Component::Normal("modules".as_ref()))
    {
        return Err(module_path_error(
            declaration,
            "must be located below the project `modules/` directory",
        ));
    }
    let bytes = read_isolated_file(project_root, &declaration.value, MAX_MANIFEST_BYTES)
        .map_err(|reason| module_path_error(declaration, &reason))?;
    let source = std::str::from_utf8(&bytes)
        .map_err(|error| module_path_error(declaration, &format!("must be UTF-8: {error}")))?;
    let mut manifest = toml::from_str::<ModuleManifest>(source).map_err(|error| {
        Diagnostic::error(
            "AS1013",
            format!("invalid module manifest `{}`: {error}", declaration.value),
            declaration.span.clone(),
        )
    })?;
    if manifest.name.starts_with("appstruct/") {
        return Err(Diagnostic::error(
            "AS3063",
            format!(
                "local module `{}` uses the reserved `appstruct/` namespace",
                manifest.name
            ),
            declaration.span.clone(),
        ));
    }
    validate_manifest(&mut manifest).map_err(|error| {
        Diagnostic::error("AS3063", error.to_string(), declaration.span.clone())
    })?;

    let manifest_path = project_root.join(&declaration.value);
    let module_directory = manifest_path.parent().ok_or_else(|| {
        module_path_error(declaration, "does not have a containing module directory")
    })?;
    let mut total_bytes = 0_usize;
    let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
    for artifact in &manifest.artifacts {
        let content = read_isolated_file(module_directory, &artifact.source, MAX_ARTIFACT_BYTES)
            .map_err(|reason| {
                Diagnostic::error(
                    "AS1013",
                    format!(
                        "cannot load artifact `{}` for module `{}`: {reason}",
                        artifact.source, manifest.name
                    ),
                    declaration.span.clone(),
                )
            })?;
        total_bytes = total_bytes
            .checked_add(content.len())
            .ok_or_else(|| module_path_error(declaration, "artifact byte count overflowed"))?;
        if total_bytes > MAX_MODULE_ARTIFACT_BYTES {
            return Err(Diagnostic::error(
                "AS1013",
                format!(
                    "module `{}` artifacts exceed the {} byte total limit",
                    manifest.name, MAX_MODULE_ARTIFACT_BYTES
                ),
                declaration.span.clone(),
            ));
        }
        let content = String::from_utf8(content).map_err(|error| {
            Diagnostic::error(
                "AS1013",
                format!(
                    "artifact `{}` for module `{}` must be UTF-8: {error}",
                    artifact.source, manifest.name
                ),
                declaration.span.clone(),
            )
        })?;
        artifacts.push(ModuleArtifactIr {
            path: artifact.path.clone(),
            content,
        });
    }
    Ok(LoadedModule {
        manifest,
        artifacts,
    })
}

fn module_path_error(declaration: &Located<String>, reason: &str) -> Diagnostic {
    Diagnostic::error(
        "AS1013",
        format!(
            "cannot load module manifest `{}`: {reason}",
            declaration.value
        ),
        declaration.span.clone(),
    )
}

fn read_isolated_file(base: &Path, relative: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    validate_relative_path(relative).map_err(str::to_owned)?;
    let mut current = base.to_path_buf();
    let components = Path::new(relative).components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(component) = component else {
            return Err("path contains an unsupported component".to_owned());
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| format!("cannot access `{relative}`: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("`{relative}` contains a symbolic link"));
        }
        let last = index + 1 == components.len();
        if (!last && !metadata.is_dir()) || (last && !metadata.is_file()) {
            return Err(format!("`{relative}` is not a regular file"));
        }
        if last && metadata.len() > max_bytes as u64 {
            return Err(format!("`{relative}` exceeds the {max_bytes} byte limit"));
        }
    }
    let canonical_base = fs::canonicalize(base)
        .map_err(|error| format!("cannot resolve module directory: {error}"))?;
    let canonical = fs::canonicalize(&current)
        .map_err(|error| format!("cannot resolve `{relative}`: {error}"))?;
    if !canonical.starts_with(canonical_base) {
        return Err(format!("`{relative}` escapes its module directory"));
    }
    let bytes = fs::read(&current).map_err(|error| format!("cannot read `{relative}`: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!("`{relative}` exceeds the {max_bytes} byte limit"));
    }
    Ok(bytes)
}

pub(crate) fn resolve_modules_for_app(
    auth: &AuthIr,
    tenant: &TenantIr,
    audit: &AuditIr,
    mail: &MailIr,
    jobs: &JobsIr,
    file: &FileIr,
    local_modules: Vec<LoadedModule>,
) -> Result<Vec<ResolvedModule>, ModuleGraphError> {
    let mut manifests = official_manifests(auth, tenant, audit, mail, jobs, file);
    manifests.extend(local_modules.iter().map(|module| module.manifest.clone()));
    let resolved = resolve_modules(manifests)?;
    let local_artifacts = local_modules
        .into_iter()
        .map(|module| (module.manifest.name, module.artifacts))
        .collect::<BTreeMap<_, _>>();
    Ok(resolved
        .into_iter()
        .map(|module| {
            let name = module.manifest.name;
            let origin = if local_artifacts.contains_key(&name) {
                ModuleOrigin::Local
            } else {
                ModuleOrigin::Official
            };
            let artifacts = local_artifacts.get(&name).cloned().unwrap_or_default();
            ResolvedModule {
                name,
                version: module.manifest.version,
                origin,
                provides: module.manifest.provides,
                requires: module.manifest.requires,
                startup_order: module.startup_order,
                artifacts,
            }
        })
        .collect())
}

fn official_manifests(
    auth: &AuthIr,
    tenant: &TenantIr,
    audit: &AuditIr,
    mail: &MailIr,
    jobs: &JobsIr,
    file: &FileIr,
) -> Vec<ModuleManifest> {
    let mut manifests = Vec::new();
    if auth.enabled {
        manifests.extend([
            manifest("auth", &[AUTH_IDENTITY], &[]),
            manifest("rbac", &["auth.roles"], &[AUTH_IDENTITY]),
        ]);
    }
    if tenant.enabled {
        manifests.push(manifest("tenant", &["tenant.context"], &[AUTH_IDENTITY]));
    }
    if audit.enabled {
        manifests.push(manifest("audit", &["audit.events"], &[AUTH_IDENTITY]));
    }
    if mail.enabled {
        manifests.push(manifest("mail", &[MAIL_DELIVERY], &[]));
    }
    if jobs.enabled {
        let requires = if mail.enabled {
            &[MAIL_DELIVERY][..]
        } else {
            &[]
        };
        manifests.push(manifest("jobs", &["jobs.outbox"], requires));
    }
    if file.enabled {
        manifests.push(manifest("file", &["file.storage"], &[]));
    }
    manifests
}

fn manifest(name: &str, provides: &[&str], requires: &[&str]) -> ModuleManifest {
    ModuleManifest::new(
        format!("appstruct/{name}"),
        env!("CARGO_PKG_VERSION"),
        provides.iter().copied(),
        requires.iter().copied(),
    )
}
