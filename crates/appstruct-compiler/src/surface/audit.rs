use super::SurfaceAudit;
use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string,
};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceAudit, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceAudit::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("audit") else {
        return Ok(SurfaceAudit::default());
    };
    let audit = expect_mapping(&entry.value, "`modules.audit`")?;
    ensure_known_keys(audit, &["enabled", "reader_roles"], "`modules.audit`")?;
    let enabled = audit
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.audit.enabled`"))
        .transpose()?
        .unwrap_or(true);
    let reader_roles = audit
        .get("reader_roles")
        .map(|value| {
            expect_sequence(&value.value, "`modules.audit.reader_roles`")?
                .iter()
                .map(|role| expect_string(role, "audit reader role"))
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(SurfaceAudit {
        enabled,
        reader_roles,
        span: Some(entry.value.span.clone()),
    })
}
