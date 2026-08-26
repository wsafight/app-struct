use super::SurfacePreset;
use super::value::{ensure_known_keys, expect_mapping, expect_string, expect_u64, required};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<Option<SurfacePreset>, Diagnostic> {
    let Some(entry) = entry else { return Ok(None) };
    let preset = expect_mapping(&entry.value, "`preset`")?;
    ensure_known_keys(preset, &["name", "version"], "`preset`")?;
    let name = required(preset, "name", &entry.value.span)?;
    let version = required(preset, "version", &entry.value.span)?;
    Ok(Some(SurfacePreset {
        name: expect_string(&name.value, "`preset.name`")?,
        version: expect_u64(&version.value, "`preset.version`")?,
    }))
}
