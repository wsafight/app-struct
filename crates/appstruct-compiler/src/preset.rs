mod lock;

use crate::surface::{SurfacePreset, SurfaceRoot};
use crate::yaml::{self, MappingEntry, Node, NodeKind};
use appstruct_ir::Diagnostic;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::Path;

const SAAS_NAME: &str = "appstruct/saas";
const SAAS_VERSION: u64 = 1;
pub(super) const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");
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
    pub defaults: &'static str,
}

#[must_use]
pub fn preset_info(name: &str, version: u64) -> Option<PresetInfo> {
    supported(name, version).then(|| PresetInfo {
        name: SAAS_NAME,
        version: SAAS_VERSION,
        digest: preset_digest(),
        modules: MODULES,
        defaults: EXPANDED,
    })
}

pub(crate) fn preset_digest() -> String {
    format!("sha256:{:x}", Sha256::digest(EXPANDED.as_bytes()))
}

/// Build the canonical project lock used by official templates.
#[must_use]
pub fn project_lock(template: &str, preset: Option<(&str, u64)>) -> Option<String> {
    lock::source(template, preset)
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
    lock::validate(project, root)
}

pub(crate) fn render_expanded_modules(modules: &MappingEntry) -> String {
    let mut output = "modules:\n".to_owned();
    render_node(&modules.value, 2, &mut output);
    output
}

fn supported(name: &str, version: u64) -> bool {
    name == SAAS_NAME && version == SAAS_VERSION
}

fn render_node(node: &Node, indent: usize, output: &mut String) {
    match &node.kind {
        NodeKind::Mapping(entries) => {
            for (key, entry) in entries {
                let _ = write!(output, "{}{key}:", " ".repeat(indent));
                match entry.value.kind {
                    NodeKind::Scalar { .. } => {
                        output.push(' ');
                        render_scalar(&entry.value, output);
                        output.push('\n');
                    }
                    NodeKind::Mapping(ref entries) if entries.is_empty() => {
                        output.push_str(" {}\n");
                    }
                    NodeKind::Sequence(ref items)
                        if items
                            .iter()
                            .all(|item| matches!(item.kind, NodeKind::Scalar { .. })) =>
                    {
                        output.push_str(" [");
                        for (index, item) in items.iter().enumerate() {
                            if index > 0 {
                                output.push_str(", ");
                            }
                            render_scalar(item, output);
                        }
                        output.push_str("]\n");
                    }
                    _ => {
                        output.push('\n');
                        render_node(&entry.value, indent + 2, output);
                    }
                }
            }
        }
        NodeKind::Sequence(items) => {
            for item in items {
                let _ = write!(output, "{}- ", " ".repeat(indent));
                if matches!(item.kind, NodeKind::Scalar { .. }) {
                    render_scalar(item, output);
                    output.push('\n');
                } else {
                    output.push('\n');
                    render_node(item, indent + 2, output);
                }
            }
        }
        NodeKind::Scalar { .. } => {
            let _ = write!(output, "{}", " ".repeat(indent));
            render_scalar(node, output);
            output.push('\n');
        }
    }
}

fn render_scalar(node: &Node, output: &mut String) {
    let Some((value, plain)) = node.scalar() else {
        return;
    };
    if plain && !value.is_empty() {
        output.push_str(value);
    } else {
        output.push('"');
        for character in value.chars() {
            match character {
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                other => output.push(other),
            }
        }
        output.push('"');
    }
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
