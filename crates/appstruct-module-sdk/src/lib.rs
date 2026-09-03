//! Stable module manifests and deterministic capability graph resolution.

mod manifest;
mod registry;

pub use manifest::{
    MODULE_API_VERSION, ModuleArtifact, ModuleManifest, module_namespace, validate_manifest,
    validate_relative_path,
};
pub use registry::{
    RegistryArtifact, RegistryEnvelope, RegistryPackage, RegistryVerificationError,
    verify_registry_envelope,
};

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModule {
    pub manifest: ModuleManifest,
    pub startup_order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleGraphError {
    InvalidManifest {
        module: String,
        message: String,
    },
    DuplicateModule {
        module: String,
    },
    DuplicateProvider {
        capability: String,
        first: String,
        second: String,
    },
    MissingCapability {
        module: String,
        capability: String,
    },
    DependencyCycle {
        modules: Vec<String>,
    },
}

impl fmt::Display for ModuleGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest { module, message } => {
                write!(
                    formatter,
                    "module `{module}` has an invalid manifest: {message}"
                )
            }
            Self::DuplicateModule { module } => {
                write!(formatter, "module `{module}` is declared more than once")
            }
            Self::DuplicateProvider {
                capability,
                first,
                second,
            } => write!(
                formatter,
                "capability `{capability}` has multiple providers: `{first}` and `{second}`"
            ),
            Self::MissingCapability { module, capability } => write!(
                formatter,
                "module `{module}` requires missing capability `{capability}`"
            ),
            Self::DependencyCycle { modules } => {
                write!(
                    formatter,
                    "module dependency cycle includes {}",
                    modules.join(", ")
                )
            }
        }
    }
}

impl Error for ModuleGraphError {}

/// Resolve manifests into a stable provider-before-consumer startup order.
///
/// # Errors
///
/// Returns an error for invalid manifests, duplicate declarations or providers,
/// missing required capabilities, and dependency cycles.
///
/// # Panics
///
/// This function does not panic for a valid manifest set. Internal lookups use
/// assertions to defend the graph invariants established during resolution.
pub fn resolve_modules(
    manifests: impl IntoIterator<Item = ModuleManifest>,
) -> Result<Vec<ResolvedModule>, ModuleGraphError> {
    let mut modules = BTreeMap::new();
    for mut manifest in manifests {
        validate_manifest(&mut manifest)?;
        let name = manifest.name.clone();
        if modules.insert(name.clone(), manifest).is_some() {
            return Err(ModuleGraphError::DuplicateModule { module: name });
        }
    }

    let providers = capability_providers(&modules)?;
    let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    let mut consumers = BTreeMap::<String, BTreeSet<String>>::new();
    for (name, manifest) in &modules {
        let required = manifest
            .requires
            .iter()
            .map(|capability| {
                providers.get(capability).cloned().ok_or_else(|| {
                    ModuleGraphError::MissingCapability {
                        module: name.clone(),
                        capability: capability.clone(),
                    }
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        for provider in required.iter().filter(|provider| *provider != name) {
            consumers
                .entry(provider.clone())
                .or_default()
                .insert(name.clone());
        }
        dependencies.insert(
            name.clone(),
            required
                .into_iter()
                .filter(|provider| provider != name)
                .collect(),
        );
    }

    let mut ready = dependencies
        .iter()
        .filter_map(|(name, required)| required.is_empty().then_some(name.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(modules.len());
    while let Some(name) = ready.pop_first() {
        let manifest = modules.get(&name).expect("ready module exists").clone();
        let startup_order =
            u32::try_from(ordered.len()).map_err(|_| ModuleGraphError::InvalidManifest {
                module: name.clone(),
                message: "too many modules to represent startup order".to_owned(),
            })?;
        ordered.push(ResolvedModule {
            manifest,
            startup_order,
        });
        for consumer in consumers.get(&name).into_iter().flatten() {
            let required = dependencies
                .get_mut(consumer)
                .expect("consumer dependency set exists");
            required.remove(&name);
            if required.is_empty() {
                ready.insert(consumer.clone());
            }
        }
    }

    if ordered.len() != modules.len() {
        let resolved = ordered
            .iter()
            .map(|module| module.manifest.name.as_str())
            .collect::<BTreeSet<_>>();
        let cycle = modules
            .keys()
            .filter(|name| !resolved.contains(name.as_str()))
            .cloned()
            .collect();
        return Err(ModuleGraphError::DependencyCycle { modules: cycle });
    }
    Ok(ordered)
}

fn capability_providers(
    modules: &BTreeMap<String, ModuleManifest>,
) -> Result<BTreeMap<String, String>, ModuleGraphError> {
    let mut providers = BTreeMap::new();
    for (name, manifest) in modules {
        for capability in &manifest.provides {
            if let Some(first) = providers.insert(capability.clone(), name.clone()) {
                return Err(ModuleGraphError::DuplicateProvider {
                    capability: capability.clone(),
                    first,
                    second: name.clone(),
                });
            }
        }
    }
    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::{ModuleGraphError, ModuleManifest, resolve_modules};

    fn manifest(name: &str, provides: &[&str], requires: &[&str]) -> ModuleManifest {
        ModuleManifest::new(
            name,
            "1",
            provides.iter().copied(),
            requires.iter().copied(),
        )
    }

    #[test]
    fn resolves_provider_before_consumer_deterministically() {
        let resolved = resolve_modules([
            manifest("audit", &["audit.events"], &["auth.identity"]),
            manifest("mail", &["mail.delivery"], &[]),
            manifest("auth", &["auth.identity"], &[]),
        ])
        .unwrap();
        let names = resolved
            .iter()
            .map(|module| module.manifest.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["auth", "audit", "mail"]);
        assert_eq!(resolved[1].startup_order, 1);
    }

    #[test]
    fn rejects_missing_capabilities() {
        let error = resolve_modules([manifest("tenant", &["tenant.context"], &["auth.identity"])])
            .unwrap_err();
        assert!(matches!(
            error,
            ModuleGraphError::MissingCapability { capability, .. }
                if capability == "auth.identity"
        ));
    }

    #[test]
    fn rejects_invalid_and_duplicate_manifests() {
        let invalid = resolve_modules([manifest("", &["identity"], &[])]).unwrap_err();
        assert!(matches!(invalid, ModuleGraphError::InvalidManifest { .. }));

        let duplicate = resolve_modules([
            manifest("auth", &["identity"], &[]),
            manifest("auth", &["roles"], &[]),
        ])
        .unwrap_err();
        assert_eq!(
            duplicate,
            ModuleGraphError::DuplicateModule {
                module: "auth".to_owned()
            }
        );
    }

    #[test]
    fn rejects_duplicate_providers() {
        let error = resolve_modules([
            manifest("first", &["storage"], &[]),
            manifest("second", &["storage"], &[]),
        ])
        .unwrap_err();
        assert!(matches!(error, ModuleGraphError::DuplicateProvider { .. }));
    }

    #[test]
    fn rejects_dependency_cycles() {
        let error = resolve_modules([
            manifest("first", &["first"], &["second"]),
            manifest("second", &["second"], &["first"]),
        ])
        .unwrap_err();
        assert_eq!(
            error,
            ModuleGraphError::DependencyCycle {
                modules: vec!["first".to_owned(), "second".to_owned()]
            }
        );
    }

    #[test]
    fn graph_errors_display_stable_messages() {
        assert!(
            ModuleGraphError::InvalidManifest {
                module: "auth".to_owned(),
                message: "bad".to_owned(),
            }
            .to_string()
            .contains("invalid manifest")
        );
        assert!(
            ModuleGraphError::DuplicateModule {
                module: "auth".to_owned(),
            }
            .to_string()
            .contains("more than once")
        );
        assert!(
            ModuleGraphError::DuplicateProvider {
                capability: "storage".to_owned(),
                first: "a".to_owned(),
                second: "b".to_owned(),
            }
            .to_string()
            .contains("multiple providers")
        );
        assert!(
            ModuleGraphError::MissingCapability {
                module: "tenant".to_owned(),
                capability: "auth.identity".to_owned(),
            }
            .to_string()
            .contains("missing capability")
        );
        assert!(
            ModuleGraphError::DependencyCycle {
                modules: vec!["a".to_owned(), "b".to_owned()],
            }
            .to_string()
            .contains("cycle")
        );
    }

    #[test]
    fn self_provided_capabilities_do_not_create_dependencies() {
        let resolved =
            resolve_modules([manifest("auth", &["auth.identity"], &["auth.identity"])]).unwrap();
        assert_eq!(resolved[0].manifest.name, "auth");
        assert_eq!(resolved[0].startup_order, 0);
    }
}
