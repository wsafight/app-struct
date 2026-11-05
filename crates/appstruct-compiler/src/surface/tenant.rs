use super::SurfaceTenant;
use super::value::{ensure_known_keys, expect_bool, expect_mapping};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceTenant, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceTenant::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("tenant") else {
        return Ok(SurfaceTenant::default());
    };
    let tenant = expect_mapping(&entry.value, "`modules.tenant`")?;
    ensure_known_keys(tenant, &["enabled", "sortable_indexes"], "`modules.tenant`")?;
    let enabled = tenant
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.tenant.enabled`"))
        .transpose()?
        .unwrap_or(true);
    let sortable_indexes = tenant
        .get("sortable_indexes")
        .map(|value| expect_bool(&value.value, "`modules.tenant.sortable_indexes`"))
        .transpose()?
        .unwrap_or(false);
    Ok(SurfaceTenant {
        enabled,
        sortable_indexes,
        span: Some(entry.value.span.clone()),
    })
}
