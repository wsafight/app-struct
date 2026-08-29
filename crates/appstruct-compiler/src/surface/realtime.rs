use super::SurfaceRealtime;
use super::value::{ensure_known_keys, expect_bool, expect_mapping, expect_u64};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceRealtime, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceRealtime::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("realtime") else {
        return Ok(SurfaceRealtime::default());
    };
    let realtime = expect_mapping(&entry.value, "`modules.realtime`")?;
    ensure_known_keys(
        realtime,
        &["enabled", "heartbeat_seconds", "presence_ttl_seconds"],
        "`modules.realtime`",
    )?;
    let enabled = realtime
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.realtime.enabled`"))
        .transpose()?
        .unwrap_or(true);
    Ok(SurfaceRealtime {
        enabled,
        heartbeat_seconds: number(realtime.get("heartbeat_seconds"), "realtime heartbeat")?,
        presence_ttl_seconds: number(
            realtime.get("presence_ttl_seconds"),
            "realtime presence TTL",
        )?,
        span: Some(entry.value.span.clone()),
    })
}

fn number(
    entry: Option<&MappingEntry>,
    context: &str,
) -> Result<Option<super::Located<u64>>, Diagnostic> {
    entry
        .map(|entry| expect_u64(&entry.value, context))
        .transpose()
}
