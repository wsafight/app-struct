use super::{MODULE_VERSION, preset_info};
use crate::loading::synthetic_span;
use crate::surface::SurfaceRoot;
use appstruct_ir::Diagnostic;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

pub(super) fn source(template: &str, preset: Option<(&str, u64)>) -> Option<String> {
    let mut output = format!(
        "lock_version = 1\nappstruct = {:?}\n\n[template]\nname = {template:?}\nversion = {:?}\n",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_VERSION"),
    );
    let Some((name, version)) = preset else {
        return Some(output);
    };
    let info = preset_info(name, version)?;
    let _ = write!(
        output,
        "\n[preset]\nname = {:?}\nversion = {}\ndigest = {:?}\n\n[modules]\n",
        info.name, info.version, info.digest,
    );
    for module in info.modules {
        let _ = writeln!(output, "{module} = {MODULE_VERSION:?}");
    }
    Some(output)
}

pub(super) fn validate(project: &Path, root: &SurfaceRoot) -> Vec<Diagnostic> {
    let Some(selected) = &root.preset else {
        return Vec::new();
    };
    let Some(info) = preset_info(&selected.name.value, selected.version.value) else {
        return vec![
            Diagnostic::error(
                "AS3058",
                format!(
                    "unsupported preset `{}@{}`",
                    selected.name.value, selected.version.value
                ),
                selected.name.span.clone(),
            )
            .with_help("this compiler supports `appstruct/saas` version 1"),
        ];
    };
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
    let lock = match toml::from_str::<ProjectLock>(&source) {
        Ok(lock) => lock,
        Err(error) => {
            return vec![lock_error(
                "AS3059",
                format!("invalid preset lock `appstruct.lock`: {error}"),
            )];
        }
    };
    validate_contract(&lock, &info)
}

fn validate_contract(lock: &ProjectLock, info: &super::PresetInfo) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if lock.lock_version != 1 || lock.appstruct != env!("CARGO_PKG_VERSION") {
        diagnostics.push(lock_error(
            "AS3060",
            "preset lock version or AppStruct version does not match this compiler",
        ));
    }
    if lock.preset.as_ref().is_none_or(|preset| {
        preset.name != info.name || preset.version != info.version || preset.digest != info.digest
    }) {
        diagnostics.push(lock_error(
            "AS3060",
            format!(
                "locked preset name, version, or digest does not match `{}@{}`",
                info.name, info.version
            ),
        ));
    }
    let expected_modules = info
        .modules
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
