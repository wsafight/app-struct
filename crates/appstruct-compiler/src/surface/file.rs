use super::SurfaceFile;
use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string, expect_u64,
};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceFile, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceFile::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("file") else {
        return Ok(SurfaceFile::default());
    };
    let file = expect_mapping(&entry.value, "`modules.file`")?;
    ensure_known_keys(
        file,
        &[
            "enabled",
            "provider",
            "local_root",
            "max_bytes",
            "allowed_content_types",
        ],
        "`modules.file`",
    )?;
    let enabled = file
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.file.enabled`"))
        .transpose()?
        .unwrap_or(true);
    let provider = file
        .get("provider")
        .map(|value| expect_string(&value.value, "`modules.file.provider`"))
        .transpose()?;
    let local_root = file
        .get("local_root")
        .map(|value| expect_string(&value.value, "`modules.file.local_root`"))
        .transpose()?;
    let max_bytes = file
        .get("max_bytes")
        .map(|value| expect_u64(&value.value, "`modules.file.max_bytes`"))
        .transpose()?;
    let allowed_content_types = file
        .get("allowed_content_types")
        .map(|value| {
            expect_sequence(&value.value, "`modules.file.allowed_content_types`")?
                .iter()
                .map(|item| expect_string(item, "allowed file content type"))
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(SurfaceFile {
        enabled,
        provider,
        local_root,
        max_bytes,
        allowed_content_types,
        span: Some(entry.value.span.clone()),
    })
}
