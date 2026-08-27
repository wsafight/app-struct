use super::{MODULE_VERSION, preset_info};
use crate::loading::synthetic_span;
use crate::surface::SurfaceRoot;
use appstruct_ir::Diagnostic;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLayout {
    LegacyGeneratedBackend = 1,
    CompositionRoot = 2,
}

pub(super) fn source(
    template: &str,
    preset: Option<(&str, u64)>,
    layout: ProjectLayout,
) -> Option<String> {
    let mut output = format!(
        "lock_version = 1\nproject_layout_version = {}\nappstruct = {:?}\n\n[template]\nname = {template:?}\nversion = {:?}\n",
        layout as u64,
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

pub(super) fn layout(project: &Path) -> Result<ProjectLayout, Diagnostic> {
    let path = project.join("appstruct.lock");
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProjectLayout::LegacyGeneratedBackend);
        }
        Err(error) => {
            return Err(lock_error(
                "AS3064",
                format!("cannot read project layout from `appstruct.lock`: {error}"),
            ));
        }
    };
    let lock = toml::from_str::<ProjectLock>(&source).map_err(|error| {
        lock_error(
            "AS3064",
            format!("invalid project lock `appstruct.lock`: {error}"),
        )
    })?;
    if lock.lock_version != 1 || lock.appstruct != env!("CARGO_PKG_VERSION") {
        return Err(lock_error(
            "AS3064",
            "project lock version or AppStruct version does not match this compiler; run `appstruct update`",
        ));
    }
    parse_layout(lock.project_layout_version).ok_or_else(|| {
        lock_error(
            "AS3064",
            "project lock has no supported `project_layout_version`; run `appstruct update`",
        )
    })
}

pub(super) fn validate(project: &Path, root: &SurfaceRoot) -> Vec<Diagnostic> {
    if let Err(diagnostic) = layout(project) {
        return vec![diagnostic];
    }
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

pub(super) fn updated_source(project: &Path, root: &SurfaceRoot) -> Result<String, Diagnostic> {
    let (template, layout) = read_update_metadata(project)?;
    let selected = root
        .preset
        .as_ref()
        .map(|preset| (preset.name.value.as_str(), preset.version.value));
    source(&template, selected, layout).ok_or_else(|| {
        let preset = root
            .preset
            .as_ref()
            .expect("only presets can be unsupported");
        Diagnostic::error(
            "AS3058",
            format!(
                "unsupported preset `{}@{}`",
                preset.name.value, preset.version.value
            ),
            preset.name.span.clone(),
        )
        .with_help("this compiler supports `appstruct/saas` version 1")
    })
}

fn read_update_metadata(project: &Path) -> Result<(String, ProjectLayout), Diagnostic> {
    let path = project.join("appstruct.lock");
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(("custom".to_owned(), infer_legacy_layout(project)?));
        }
        Err(error) => {
            return Err(lock_error(
                "AS3059",
                format!("cannot read preset lock `appstruct.lock`: {error}"),
            ));
        }
    };
    let lock = toml::from_str::<ProjectLock>(&source).map_err(|error| {
        lock_error(
            "AS3059",
            format!("invalid preset lock `appstruct.lock`: {error}"),
        )
    })?;
    let template = lock
        .template
        .map_or_else(|| "custom".to_owned(), |template| template.name);
    let layout = match lock.project_layout_version {
        Some(version) => parse_layout(Some(version)).ok_or_else(|| {
            lock_error(
                "AS3064",
                format!("unsupported project layout version `{version}`"),
            )
        })?,
        None => infer_legacy_layout(project)?,
    };
    Ok((template, layout))
}

fn infer_legacy_layout(project: &Path) -> Result<ProjectLayout, Diagnostic> {
    let manifest = project.join("app/backend/Cargo.toml").is_file();
    let library = project.join("app/backend/src/lib.rs").is_file();
    match (manifest, library) {
        (false, false) => Ok(ProjectLayout::LegacyGeneratedBackend),
        (true, true) => Ok(ProjectLayout::CompositionRoot),
        _ => Err(lock_error(
            "AS3064",
            "cannot migrate a partial `app/backend` layout; expected both `Cargo.toml` and `src/lib.rs`",
        )),
    }
}

fn parse_layout(version: Option<u64>) -> Option<ProjectLayout> {
    match version {
        Some(1) => Some(ProjectLayout::LegacyGeneratedBackend),
        Some(2) => Some(ProjectLayout::CompositionRoot),
        Some(_) | None => None,
    }
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
    project_layout_version: Option<u64>,
    appstruct: String,
    template: Option<LockedTemplate>,
    preset: Option<LockedPreset>,
    #[serde(default)]
    modules: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct LockedTemplate {
    name: String,
}

#[derive(Debug, Deserialize)]
struct LockedPreset {
    name: String,
    version: u64,
    digest: String,
}
