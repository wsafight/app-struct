//! `AppStruct` configuration loading, validation, and normalization.

mod access;
mod audit;
mod auth;
mod extension;
mod field;
mod field_options;
mod file;
mod jobs;
mod lint;
mod loading;
mod lower;
mod mail;
mod naming;
mod preset;
mod surface;
mod tenant;
mod validation;
mod yaml;

pub use loading::discover_project;
pub use preset::{PresetInfo, preset_info, project_lock};

/// Draft 2020-12 schema for root and domain App Spec YAML documents.
pub const APP_SPEC_SCHEMA: &str = include_str!("../schema/appstruct.schema.json");

#[derive(Clone, Debug)]
pub struct CompileReport {
    pub ir: AppIr,
    pub diagnostics: Vec<Diagnostic>,
}

use appstruct_ir::{AppIr, Diagnostic, SourceSpan};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Compile a project root into normalized, deterministically ordered IR.
///
/// # Errors
///
/// Returns all diagnostics found during path resolution and semantic validation. YAML shape errors
/// are reported per source before semantic validation starts.
pub fn compile_project(project_root: &Path) -> Result<AppIr, Vec<Diagnostic>> {
    compile_project_report(project_root).map(|report| report.ir)
}

/// Compile a project and retain non-fatal diagnostics for CLI and CI policy.
///
/// # Errors
///
/// Returns all fatal diagnostics found during loading and semantic validation.
pub fn compile_project_report(project_root: &Path) -> Result<CompileReport, Vec<Diagnostic>> {
    let root = canonical_project_root(project_root)?;
    let root_node = loading::load_yaml(&root, &root.join("appstruct.yaml"))?;
    let surface_root = surface::decode_root(&root_node).map_err(|error| vec![error])?;
    let mut diagnostics = validation::validate_root(&surface_root);
    diagnostics.extend(preset::validate_lock(&root, &surface_root));
    let mut canonical_includes = BTreeMap::<PathBuf, SourceSpan>::new();
    let mut application = surface::SurfaceDomain::default();

    for include in &surface_root.includes {
        let include_path = match loading::resolve_include(&root, include) {
            Ok(path) => path,
            Err(error) => {
                diagnostics.push(error);
                continue;
            }
        };
        if let Some(first_span) =
            canonical_includes.insert(include_path.clone(), include.span.clone())
        {
            diagnostics.push(
                Diagnostic::error(
                    "AS1009",
                    format!("duplicate include `{}`", include.value),
                    include.span.clone(),
                )
                .with_secondary(first_span, "first included here"),
            );
            continue;
        }

        match loading::load_yaml(&root, &include_path)
            .and_then(|node| surface::decode_domain(&node).map_err(|error| vec![error]))
        {
            Ok(domain) => application.extend(domain),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let warnings = lint::warnings(&application);
    lower::build_ir(surface_root, application).map(|ir| CompileReport {
        ir,
        diagnostics: warnings,
    })
}

/// Return the selected preset's effective module configuration after project overrides.
///
/// # Errors
///
/// Returns root parsing, validation, or preset lock diagnostics.
pub fn expanded_preset(project_root: &Path) -> Result<Option<String>, Vec<Diagnostic>> {
    let root = canonical_project_root(project_root)?;
    let root_node = loading::load_yaml(&root, &root.join("appstruct.yaml"))?;
    let surface_root = surface::decode_root(&root_node).map_err(|error| vec![error])?;
    let mut diagnostics = validation::validate_root(&surface_root);
    diagnostics.extend(preset::validate_lock(&root, &surface_root));
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(surface_root
        .preset
        .as_ref()
        .and(surface_root.expanded_modules.as_ref())
        .map(preset::render_expanded_modules))
}

/// Build the canonical lock candidate for an explicit project update.
///
/// Existing version and digest fields may be stale, but the lock must remain readable so its
/// template identity can be preserved. The returned source must still pass a complete staged
/// project compilation before it replaces the current lock.
///
/// # Errors
///
/// Returns root configuration, lock parsing, or unsupported preset diagnostics.
pub fn updated_project_lock(project_root: &Path) -> Result<String, Vec<Diagnostic>> {
    let root = canonical_project_root(project_root)?;
    let root_node = loading::load_yaml(&root, &root.join("appstruct.yaml"))?;
    let surface_root = surface::decode_root(&root_node).map_err(|error| vec![error])?;
    let diagnostics = validation::validate_root(&surface_root);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    preset::updated_lock(&root, &surface_root).map_err(|error| vec![error])
}

fn canonical_project_root(project_root: &Path) -> Result<PathBuf, Vec<Diagnostic>> {
    fs::canonicalize(project_root).map_err(|error| {
        vec![Diagnostic::error(
            "AS1008",
            format!(
                "cannot access project root `{}`: {error}",
                project_root.display()
            ),
            loading::synthetic_span(&project_root.to_string_lossy()),
        )]
    })
}
