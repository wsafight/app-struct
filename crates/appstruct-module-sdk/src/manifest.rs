use crate::ModuleGraphError;
use serde::{Deserialize, Serialize};

/// Current local module manifest format version.
pub const MODULE_API_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleManifest {
    pub api_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<ModuleArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleArtifact {
    pub path: String,
    pub source: String,
}

impl ModuleManifest {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        provides: impl IntoIterator<Item = impl Into<String>>,
        requires: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            api_version: MODULE_API_VERSION,
            name: name.into(),
            version: version.into(),
            provides: provides.into_iter().map(Into::into).collect(),
            requires: requires.into_iter().map(Into::into).collect(),
            artifacts: Vec::new(),
        }
    }
}

/// Validate and normalize a module manifest before graph resolution.
///
/// # Errors
///
/// Returns an error for unsupported API versions, unsafe identifiers or paths, and duplicate
/// artifact destinations.
pub fn validate_manifest(manifest: &mut ModuleManifest) -> Result<(), ModuleGraphError> {
    if manifest.api_version != MODULE_API_VERSION {
        return Err(invalid(
            manifest,
            &format!(
                "unsupported api_version {}; expected {MODULE_API_VERSION}",
                manifest.api_version
            ),
        ));
    }
    if !is_dotted_identifier(&manifest.name, '/') {
        return Err(invalid(
            manifest,
            "name must contain lowercase ASCII path segments separated by `/`",
        ));
    }
    if manifest.version.is_empty()
        || !manifest
            .version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(invalid(
            manifest,
            "version must contain only ASCII letters, digits, `.`, `-`, or `+`",
        ));
    }
    if manifest
        .provides
        .iter()
        .chain(&manifest.requires)
        .any(|capability| !is_dotted_identifier(capability, '.'))
    {
        return Err(invalid(
            manifest,
            "capabilities must contain lowercase ASCII segments separated by `.`",
        ));
    }
    manifest.provides.sort();
    manifest.provides.dedup();
    manifest.requires.sort();
    manifest.requires.dedup();
    manifest
        .artifacts
        .sort_by(|left, right| left.path.cmp(&right.path));
    let mut previous = None;
    for artifact in &manifest.artifacts {
        validate_relative_path(&artifact.path)
            .map_err(|message| invalid(manifest, &format!("artifact path {message}")))?;
        validate_relative_path(&artifact.source)
            .map_err(|message| invalid(manifest, &format!("artifact source {message}")))?;
        if previous == Some(artifact.path.as_str()) {
            return Err(invalid(
                manifest,
                &format!(
                    "artifact path `{}` is declared more than once",
                    artifact.path
                ),
            ));
        }
        previous = Some(&artifact.path);
    }
    Ok(())
}

/// Return a portable, collision-free generated namespace for a validated module name.
///
/// # Errors
///
/// Returns an error when `name` is not a valid module identifier.
pub fn module_namespace(name: &str) -> Result<String, &'static str> {
    if !is_dotted_identifier(name, '/') {
        return Err("is not a valid module name");
    }
    Ok(name.replace('/', "+"))
}

/// Validate a portable relative path used by module artifact declarations.
///
/// # Errors
///
/// Returns a short reason when a path is empty, absolute, traverses upward, or is not portable
/// across supported hosts.
pub fn validate_relative_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("cannot be empty");
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err("must be a relative file path");
    }
    if path.contains('\\') || path.contains(':') || path.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("must use portable `/`-separated components");
    }
    if path
        .split('/')
        .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err("cannot contain empty, `.` or `..` components");
    }
    Ok(())
}

fn is_dotted_identifier(value: &str, separator: char) -> bool {
    !value.is_empty()
        && value.split(separator).all(|segment| {
            let mut bytes = segment.bytes();
            bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
                && bytes.all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
        })
}

fn invalid(manifest: &ModuleManifest, message: &str) -> ModuleGraphError {
    ModuleGraphError::InvalidManifest {
        module: manifest.name.clone(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{MODULE_API_VERSION, ModuleArtifact, ModuleManifest, module_namespace};
    use crate::{ModuleGraphError, resolve_modules};

    #[test]
    fn rejects_unsupported_versions_and_unsafe_artifact_paths() {
        let mut future = ModuleManifest::new(
            "local/example",
            "1",
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        );
        future.api_version = MODULE_API_VERSION + 1;
        assert!(matches!(
            resolve_modules([future]),
            Err(ModuleGraphError::InvalidManifest { .. })
        ));

        let mut traversal = ModuleManifest::new(
            "local/example",
            "1",
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        );
        traversal.artifacts.push(ModuleArtifact {
            path: "../outside.txt".to_owned(),
            source: "assets/outside.txt".to_owned(),
        });
        assert!(matches!(
            resolve_modules([traversal]),
            Err(ModuleGraphError::InvalidManifest { .. })
        ));
    }

    #[test]
    fn creates_collision_free_namespaces() {
        assert_eq!(module_namespace("local/example").unwrap(), "local+example");
        assert_ne!(
            module_namespace("local/example-one").unwrap(),
            module_namespace("local-example/one").unwrap()
        );
    }
}
