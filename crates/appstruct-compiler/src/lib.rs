//! `AppStruct` configuration loading, validation, and normalization.

mod access;
mod field;
mod loading;
mod lower;
mod naming;
mod surface;
mod validation;
mod yaml;

pub use loading::discover_project;

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
    let root = canonical_project_root(project_root)?;
    let root_node = loading::load_yaml(&root, &root.join("appstruct.yaml"))?;
    let surface_root = surface::decode_root(&root_node).map_err(|error| vec![error])?;
    let mut diagnostics = validation::validate_root(&surface_root);
    let mut canonical_includes = BTreeMap::<PathBuf, SourceSpan>::new();
    let mut entities = Vec::new();

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
            Ok(domain) => entities.extend(domain.entities),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if diagnostics.is_empty() {
        lower::build_ir(surface_root, entities)
    } else {
        Err(diagnostics)
    }
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
