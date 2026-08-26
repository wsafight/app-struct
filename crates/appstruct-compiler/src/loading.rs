use crate::surface::Located;
use crate::yaml;
use appstruct_ir::{Diagnostic, SourceSpan};
use std::fs;
use std::path::{Path, PathBuf};

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
