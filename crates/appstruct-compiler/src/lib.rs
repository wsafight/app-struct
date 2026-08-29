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
mod module;
mod module_registry;
mod naming;
mod preset;
mod realtime;
mod surface;
mod tenant;
mod validation;
mod webhooks;
mod yaml;

pub use loading::discover_project;
pub use preset::{
    CURRENT_PROJECT_LAYOUT_VERSION, PresetInfo, ProjectLayout, preset_info, project_layout,
    project_lock,
};

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
    let surface_root = surface::decode_root(&root_node)?;
    let mut diagnostics = validation::validate_root(&surface_root);
    diagnostics.extend(preset::validate_lock(&root, &surface_root));
    let (local_modules, module_diagnostics) =
        module::load_local_modules(&root, &surface_root.module_manifests);
    if module_diagnostics.is_empty() {
        diagnostics.extend(preset::validate_local_module_lock(&root, &local_modules));
    }
    diagnostics.extend(module_diagnostics);
    let (remote_modules, remote_diagnostics) = module_registry::load(&root);
    diagnostics.extend(remote_diagnostics);
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
            .and_then(|node| surface::decode_domain(&node))
        {
            Ok(domain) => application.extend(domain),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let warnings = lint::warnings(&application);
    let mut modules = local_modules;
    modules.extend(remote_modules);
    lower::build_ir(surface_root, application, modules).map(|ir| CompileReport {
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
    let surface_root = surface::decode_root(&root_node)?;
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
    let surface_root = surface::decode_root(&root_node)?;
    let diagnostics = validation::validate_root(&surface_root);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let (local_modules, module_diagnostics) =
        module::load_local_modules(&root, &surface_root.module_manifests);
    if !module_diagnostics.is_empty() {
        return Err(module_diagnostics);
    }
    preset::updated_lock(&root, &surface_root, &local_modules).map_err(|error| vec![error])
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
