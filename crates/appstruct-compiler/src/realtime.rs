use crate::surface::SurfaceRealtime;
use appstruct_ir::{AuthIr, Diagnostic, RealtimeIr, SourceSpan};

pub(crate) fn lower_realtime(
    realtime: &SurfaceRealtime,
    auth: &AuthIr,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> RealtimeIr {
    if !realtime.enabled {
        return RealtimeIr::default();
    }
    if !auth.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3080",
            "realtime requires the auth module",
            realtime.span.as_ref().unwrap_or(fallback).clone(),
        ));
    }
    let heartbeat = ranged(
        realtime.heartbeat_seconds.as_ref(),
        15,
        5,
        300,
        "heartbeat_seconds",
        "AS3081",
        diagnostics,
    );
    let ttl = ranged(
        realtime.presence_ttl_seconds.as_ref(),
        45,
        15,
        900,
        "presence_ttl_seconds",
        "AS3082",
        diagnostics,
    );
    if ttl <= heartbeat {
        diagnostics.push(Diagnostic::error(
            "AS3083",
            "presence TTL must be greater than the heartbeat interval",
            realtime.span.as_ref().unwrap_or(fallback).clone(),
        ));
    }
    RealtimeIr {
        enabled: true,
        heartbeat_seconds: heartbeat,
        presence_ttl_seconds: ttl,
    }
}

fn ranged(
    value: Option<&crate::surface::Located<u64>>,
    default: u64,
    minimum: u64,
    maximum: u64,
    name: &str,
    code: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> u64 {
    value.map_or(default, |value| {
        if !(minimum..=maximum).contains(&value.value) {
            diagnostics.push(Diagnostic::error(
                code,
                format!("`{name}` must be between {minimum} and {maximum}"),
                value.span.clone(),
            ));
        }
        value.value.clamp(minimum, maximum)
    })
}
