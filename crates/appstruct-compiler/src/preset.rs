use crate::loading::synthetic_span;
use crate::surface::{SurfacePreset, SurfaceRoot};
use crate::yaml::{self, MappingEntry, Node, NodeKind};
use appstruct_ir::Diagnostic;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

const SAAS_NAME: &str = "appstruct/saas";
const SAAS_VERSION: u64 = 1;
const MODULE_VERSION: &str = "0.1.0";
const MODULES: &[&str] = &["audit", "auth", "file", "jobs", "mail", "rbac", "tenant"];
const EXPANDED: &str = concat!(
    "modules:\n",
    "  auth:\n",
    "    enabled: true\n",
    "    user_entity: User\n",
    "    registration: true\n",
    "    password_reset: true\n",
    "  rbac:\n",
    "    roles: [member, admin]\n",
    "    default_role: member\n",
    "  tenant:\n",
    "    enabled: true\n",
    "  audit:\n",
    "    enabled: true\n",
    "    reader_roles: [admin]\n",
    "  mail:\n",
    "    enabled: true\n",
    "    provider: capture\n",
    "    from: \"AppStruct <notifications@example.com>\"\n",
    "    templates:\n",
    "      invitation:\n",
    "        subject: \"You are invited to {{ organization_name }}\"\n",
    "        text: \"Accept your invitation to join {{ organization_name }}.\"\n",
    "      welcome:\n",
    "        subject: \"Welcome {{ recipient_name }}\"\n",
    "        text: \"Your AppStruct workspace is ready.\"\n",
    "  jobs:\n",
    "    enabled: true\n",
    "    poll_interval_ms: 250\n",
    "    lease_seconds: 30\n",
    "    queues:\n",
    "      default: { max_attempts: 5, backoff_seconds: 2 }\n",
    "      mail: { max_attempts: 8, backoff_seconds: 5 }\n",
    "  file:\n",
    "    enabled: true\n",
    "    provider: local\n",
    "    local_root: .appstruct/files\n",
    "    max_bytes: 10485760\n",
    "    allowed_content_types: [text/plain, application/json, image/png, image/jpeg]\n",
);

#[derive(Clone, Debug)]
pub struct PresetInfo {
    pub name: &'static str,
    pub version: u64,
    pub digest: String,
    pub modules: &'static [&'static str],
    pub expanded: &'static str,
}

#[must_use]
pub fn preset_info(name: &str, version: u64) -> Option<PresetInfo> {
    supported(name, version).then(|| PresetInfo {
        name: SAAS_NAME,
        version: SAAS_VERSION,
        digest: preset_digest(),
        modules: MODULES,
        expanded: EXPANDED,
    })
}

pub(crate) fn preset_digest() -> String {
    format!("sha256:{:x}", Sha256::digest(EXPANDED.as_bytes()))
}

pub(crate) fn expand_modules(
    preset: Option<&SurfacePreset>,
    overrides: Option<&MappingEntry>,
) -> Result<Option<MappingEntry>, Diagnostic> {
    let Some(preset) = preset else {
        return Ok(overrides.cloned());
    };
    if !supported(&preset.name.value, preset.version.value) {
        return Ok(overrides.cloned());
    }
    let defaults = yaml::parse("<preset appstruct/saas@1>", EXPANDED)?;
    let mut modules = defaults
        .mapping()
        .and_then(|mapping| mapping.get("modules"))
        .cloned()
        .expect("official preset must contain modules");
    if let Some(overrides) = overrides {
        merge(&mut modules.value, &overrides.value);
    }
    Ok(Some(modules))
}

fn merge(defaults: &mut Node, overrides: &Node) {
    match (&mut defaults.kind, &overrides.kind) {
        (NodeKind::Mapping(defaults), NodeKind::Mapping(overrides)) => {
            for (key, value) in overrides {
                if let Some(default) = defaults.get_mut(key) {
                    merge(&mut default.value, &value.value);
                    default.key_span = value.key_span.clone();
                } else {
                    defaults.insert(key.clone(), value.clone());
                }
            }
        }
        _ => *defaults = overrides.clone(),
    }
}

pub(crate) fn validate_lock(project: &Path, root: &SurfaceRoot) -> Vec<Diagnostic> {
    let Some(preset) = &root.preset else {
        return Vec::new();
    };
    if !supported(&preset.name.value, preset.version.value) {
        return vec![
            Diagnostic::error(
                "AS3058",
                format!(
                    "unsupported preset `{}@{}`",
                    preset.name.value, preset.version.value
                ),
                preset.name.span.clone(),
            )
            .with_help("this compiler supports `appstruct/saas` version 1"),
        ];
    }
    let path = project.join("appstruct.lock");
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            return vec![lock_error(
                "AS3059",
                format!("cannot read preset lock `appstruct.lock`: {error}"),
            )];
        }
    };
    let lock: ProjectLock = match toml::from_str(&source) {
        Ok(lock) => lock,
        Err(error) => {
            return vec![lock_error(
                "AS3059",
                format!("invalid preset lock `appstruct.lock`: {error}"),
            )];
        }
    };
    validate_lock_contract(&lock)
}

fn validate_lock_contract(lock: &ProjectLock) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if lock.lock_version != 1 || lock.appstruct != env!("CARGO_PKG_VERSION") {
        diagnostics.push(lock_error(
            "AS3060",
            "preset lock version or AppStruct version does not match this compiler",
        ));
    }
    let expected_digest = preset_digest();
    if lock.preset.as_ref().is_none_or(|preset| {
        preset.name != SAAS_NAME
            || preset.version != SAAS_VERSION
            || preset.digest != expected_digest
    }) {
        diagnostics.push(lock_error(
            "AS3060",
            "locked preset name, version, or digest does not match `appstruct/saas@1`",
        ));
    }
    let expected_modules = MODULES
        .iter()
        .map(|name| ((*name).to_owned(), MODULE_VERSION.to_owned()))
        .collect::<BTreeMap<_, _>>();
    if lock.modules != expected_modules {
        diagnostics.push(lock_error(
            "AS3061",
            "locked preset module set or version is incomplete",
        ));
    }
    diagnostics
}

fn supported(name: &str, version: u64) -> bool {
    name == SAAS_NAME && version == SAAS_VERSION
}

fn lock_error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(code, message, synthetic_span("appstruct.lock"))
}

#[derive(Debug, Deserialize)]
struct ProjectLock {
    lock_version: u64,
    appstruct: String,
    preset: Option<LockedPreset>,
    #[serde(default)]
    modules: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct LockedPreset {
    name: String,
    version: u64,
    digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_preset_digest_is_stable() {
        assert_eq!(
            preset_digest(),
            "sha256:7267c3a362b328d5b162f4536ac7115c72ab81cf9f3e17542a8d9b6eba7965c5"
        );
    }
}
