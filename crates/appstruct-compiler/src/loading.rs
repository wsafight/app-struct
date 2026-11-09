use crate::surface::Located;
use crate::yaml;
use appstruct_ir::{Diagnostic, SourceSpan};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Discover project-relative include paths, symlink dependencies and their in-project targets.
/// Missing includes remain in the result so callers can detect deletion and recreation.
///
/// # Errors
///
/// Returns diagnostics when the root YAML cannot be read or an include escapes the project.
pub fn project_include_paths(project_root: &Path) -> Result<Vec<PathBuf>, Vec<Diagnostic>> {
    let root = super::canonical_project_root(project_root)?;
    let node = load_yaml(&root, &root.join("appstruct.yaml"))?;
    let includes = node
        .mapping()
        .and_then(|mapping| mapping.get("includes"))
        .and_then(|entry| entry.value.sequence())
        .unwrap_or_default();
    let mut paths = BTreeSet::new();
    for include in includes {
        let Some((value, _)) = include.scalar() else {
            continue;
        };
        let mut relative = PathBuf::new();
        for component in Path::new(value).components() {
            match component {
                std::path::Component::Normal(part) => {
                    relative.push(part);
                    let path = root.join(&relative);
                    if fs::symlink_metadata(&path)
                        .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    {
                        paths.insert(relative.clone());
                    }
                    if let Ok(canonical) = fs::canonicalize(&path) {
                        relative = canonical
                            .strip_prefix(&root)
                            .map_err(|_| {
                                vec![Diagnostic::error(
                                    "AS1010",
                                    "include escapes the project root",
                                    include.span.clone(),
                                )]
                            })?
                            .to_path_buf();
                    }
                }
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir if relative.pop() => {}
                _ => {
                    return Err(vec![Diagnostic::error(
                        "AS1010",
                        "include paths must stay within the project root",
                        include.span.clone(),
                    )]);
                }
            }
        }
        paths.insert(relative);
    }
    Ok(paths.into_iter().collect())
}

/// Find the nearest `AppStruct` project at or above `start`.
///
/// # Errors
///
/// Returns a diagnostic if the start path cannot be resolved or no `appstruct.yaml` exists.
pub fn discover_project(start: &Path) -> Result<PathBuf, Diagnostic> {
    let start = fs::canonicalize(start).map_err(|error| {
        Diagnostic::error(
            "AS1008",
            format!("cannot access project path `{}`: {error}", start.display()),
            synthetic_span(&start.to_string_lossy()),
        )
    })?;
    let mut directory = if start.is_file() {
        start.parent().map(Path::to_path_buf)
    } else {
        Some(start)
    };
    while let Some(candidate) = directory {
        if candidate.join("appstruct.yaml").is_file() {
            return Ok(candidate);
        }
        directory = candidate.parent().map(Path::to_path_buf);
    }
    Err(Diagnostic::error(
        "AS1008",
        "could not find `appstruct.yaml` in this directory or any parent",
        synthetic_span("appstruct.yaml"),
    ))
}

pub(crate) fn load_yaml(root: &Path, path: &Path) -> Result<yaml::Node, Vec<Diagnostic>> {
    let label = relative_label(root, path);
    let source = fs::read_to_string(path).map_err(|error| {
        vec![Diagnostic::error(
            "AS1008",
            format!("cannot read `{label}`: {error}"),
            synthetic_span(&label),
        )]
    })?;
    yaml::parse(&label, &source).map_err(|error| vec![error])
}

pub(crate) fn resolve_include(
    root: &Path,
    include: &Located<String>,
) -> Result<PathBuf, Diagnostic> {
    let declared = Path::new(&include.value);
    if declared.is_absolute() {
        return Err(Diagnostic::error(
            "AS1010",
            "include paths must be relative to the project root",
            include.span.clone(),
        ));
    }
    let canonical = fs::canonicalize(root.join(declared)).map_err(|error| {
        Diagnostic::error(
            "AS1008",
            format!("cannot access include `{}`: {error}", include.value),
            include.span.clone(),
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(Diagnostic::error(
            "AS1010",
            format!("include `{}` escapes the project root", include.value),
            include.span.clone(),
        ));
    }
    if !canonical.is_file() {
        return Err(Diagnostic::error(
            "AS1008",
            format!("include `{}` is not a file", include.value),
            include.span.clone(),
        ));
    }
    Ok(canonical)
}

fn relative_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn synthetic_span(file: &str) -> SourceSpan {
    SourceSpan {
        file: file.to_owned(),
        start: 0,
        end: 0,
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 1,
    }
}
