use crate::surface::SurfaceFile;
use appstruct_ir::{Diagnostic, FileIr, FileProviderIr, SourceSpan};
use std::path::{Component, Path};

const DEFAULT_ROOT: &str = ".appstruct/files";
const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
const MAX_BYTES: u64 = 100 * 1024 * 1024;

pub(crate) fn lower_file(
    file: &SurfaceFile,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> FileIr {
    if !file.enabled {
        return disabled();
    }
    let span = file.span.as_ref().unwrap_or(fallback);
    let provider = lower_provider(file, span, diagnostics);
    let local_root = file.local_root.as_ref().map_or_else(
        || DEFAULT_ROOT.to_owned(),
        |root| {
            if !safe_relative_root(&root.value) {
                diagnostics.push(Diagnostic::error(
                    "AS3054",
                    "`modules.file.local_root` must be a safe relative path",
                    root.span.clone(),
                ));
            }
            root.value.clone()
        },
    );
    let max_bytes = file.max_bytes.as_ref().map_or(DEFAULT_MAX_BYTES, |value| {
        if !(1..=MAX_BYTES).contains(&value.value) {
            diagnostics.push(Diagnostic::error(
                "AS3055",
                format!("`modules.file.max_bytes` must be between 1 and {MAX_BYTES}"),
                value.span.clone(),
            ));
        }
        value.value.clamp(1, MAX_BYTES)
    });
    if file.allowed_content_types.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3056",
            "enabled file module requires at least one allowed content type",
            span.clone(),
        ));
    }
    let mut allowed_content_types = file
        .allowed_content_types
        .iter()
        .map(|content_type| {
            if !valid_content_type(&content_type.value) {
                diagnostics.push(Diagnostic::error(
                    "AS3057",
                    "allowed content type must be lowercase `type/subtype` or `type/*`",
                    content_type.span.clone(),
                ));
            }
            content_type.value.clone()
        })
        .collect::<Vec<_>>();
    allowed_content_types.sort();
    allowed_content_types.dedup();
    FileIr {
        enabled: true,
        provider,
        local_root,
        max_bytes,
        allowed_content_types,
    }
}

fn disabled() -> FileIr {
    FileIr {
        enabled: false,
        provider: FileProviderIr::Local,
        local_root: DEFAULT_ROOT.to_owned(),
        max_bytes: DEFAULT_MAX_BYTES,
        allowed_content_types: Vec::new(),
    }
}

fn lower_provider(
    file: &SurfaceFile,
    span: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> FileProviderIr {
    let Some(provider) = &file.provider else {
        diagnostics.push(Diagnostic::error(
            "AS3053",
            "enabled file module requires `modules.file.provider`",
            span.clone(),
        ));
        return FileProviderIr::Local;
    };
    match provider.value.as_str() {
        "local" => FileProviderIr::Local,
        "s3" => FileProviderIr::S3,
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS3053",
                "file provider must be `local` or `s3`",
                provider.span.clone(),
            ));
            FileProviderIr::Local
        }
    }
}

fn safe_relative_root(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_content_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && kind.bytes().all(valid_token)
        && (subtype == "*" || subtype.bytes().all(valid_token))
}

fn valid_token(value: u8) -> bool {
    value.is_ascii_lowercase()
        || value.is_ascii_digit()
        || matches!(
            value,
            b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
        )
}
