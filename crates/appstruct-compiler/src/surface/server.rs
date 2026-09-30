use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_scalar_string, expect_string,
};
use super::{
    SurfaceFrameOptions, SurfaceHsts, SurfaceReferrerPolicy, SurfaceSecurityHeaders, SurfaceServer,
};
use crate::yaml::MappingEntry;
use appstruct_ir::{Diagnostic, SourceSpan};
use std::collections::BTreeMap;

const SECURITY_HEADER_KEYS: &[&str] = &[
    "enabled",
    "hsts",
    "frame_options",
    "referrer_policy",
    "content_security_policy",
    "custom",
];

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceServer, Diagnostic> {
    let Some(entry) = entry else {
        return Ok(SurfaceServer::default());
    };
    let server = expect_mapping(&entry.value, "`server`")?;
    ensure_known_keys(server, &["security_headers"], "`server`")?;
    let security_headers = match server.get("security_headers") {
        Some(entry) => decode_security_headers(entry)?,
        None => SurfaceSecurityHeaders::default(),
    };
    Ok(SurfaceServer { security_headers })
}

fn decode_security_headers(entry: &MappingEntry) -> Result<SurfaceSecurityHeaders, Diagnostic> {
    let headers = expect_mapping(&entry.value, "`server.security_headers`")?;
    ensure_known_keys(headers, SECURITY_HEADER_KEYS, "`server.security_headers`")?;
    let span = entry.value.span.clone();
    let enabled = headers
        .get("enabled")
        .map(|entry| expect_bool(&entry.value, "`server.security_headers.enabled`"))
        .transpose()?
        .unwrap_or(true);
    let hsts = match headers.get("hsts") {
        Some(entry) => decode_hsts(entry)?,
        None => SurfaceHsts::Auto,
    };
    let frame_options = match headers.get("frame_options") {
        Some(entry) => decode_frame_options(entry)?,
        None => SurfaceFrameOptions::Deny,
    };
    let referrer_policy = match headers.get("referrer_policy") {
        Some(entry) => decode_referrer_policy(entry)?,
        None => SurfaceReferrerPolicy::StrictOriginWhenCrossOrigin,
    };
    let content_security_policy = match headers.get("content_security_policy") {
        Some(entry) if !is_yaml_null(&entry.value) => {
            let value = expect_string(
                &entry.value,
                "`server.security_headers.content_security_policy`",
            )?;
            validate_header_value(
                &value.value,
                "`server.security_headers.content_security_policy`",
                &value.span,
            )?;
            Some(value)
        }
        _ => None,
    };
    let custom = match headers.get("custom") {
        Some(entry) => decode_custom(entry)?,
        None => BTreeMap::new(),
    };
    Ok(SurfaceSecurityHeaders {
        enabled,
        hsts,
        frame_options,
        referrer_policy,
        content_security_policy,
        custom,
        span: Some(span),
    })
}

fn is_yaml_null(node: &crate::yaml::Node) -> bool {
    matches!(node.scalar(), Some(("null" | "Null" | "NULL" | "~", true)))
}

fn decode_hsts(entry: &MappingEntry) -> Result<SurfaceHsts, Diagnostic> {
    let value = expect_scalar_string(&entry.value, "`server.security_headers.hsts`")?;
    match value.value.as_str() {
        "auto" => Ok(SurfaceHsts::Auto),
        "off" => Ok(SurfaceHsts::Off),
        _ => value
            .value
            .parse::<u64>()
            .map(SurfaceHsts::MaxAge)
            .map_err(|_| {
                invalid_value(
                    "`server.security_headers.hsts`",
                    "`auto`, `off`, or a non-negative max-age in seconds",
                    &value.span,
                )
            }),
    }
}

fn decode_frame_options(entry: &MappingEntry) -> Result<SurfaceFrameOptions, Diagnostic> {
    let value = expect_scalar_string(&entry.value, "`server.security_headers.frame_options`")?;
    match value.value.as_str() {
        "deny" => Ok(SurfaceFrameOptions::Deny),
        "sameorigin" => Ok(SurfaceFrameOptions::SameOrigin),
        "off" => Ok(SurfaceFrameOptions::Off),
        _ => Err(invalid_value(
            "`server.security_headers.frame_options`",
            "`deny`, `sameorigin`, or `off`",
            &value.span,
        )),
    }
}

fn decode_referrer_policy(entry: &MappingEntry) -> Result<SurfaceReferrerPolicy, Diagnostic> {
    let value = expect_scalar_string(&entry.value, "`server.security_headers.referrer_policy`")?;
    match value.value.as_str() {
        "no-referrer" => Ok(SurfaceReferrerPolicy::NoReferrer),
        "same-origin" => Ok(SurfaceReferrerPolicy::SameOrigin),
        "strict-origin-when-cross-origin" => Ok(SurfaceReferrerPolicy::StrictOriginWhenCrossOrigin),
        "off" => Ok(SurfaceReferrerPolicy::Off),
        _ => Err(invalid_value(
            "`server.security_headers.referrer_policy`",
            "`no-referrer`, `same-origin`, `strict-origin-when-cross-origin`, or `off`",
            &value.span,
        )),
    }
}

fn decode_custom(entry: &MappingEntry) -> Result<BTreeMap<String, String>, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "`server.security_headers.custom`")?;
    let mut custom = BTreeMap::new();
    for (name, entry) in mapping {
        if !is_header_name(name) {
            return Err(Diagnostic::error(
                "AS1013",
                format!(
                    "`{name}` is not a valid HTTP header name; use lowercase ASCII letters, digits, and hyphens"
                ),
                entry.key_span.clone(),
            ));
        }
        let value = expect_scalar_string(
            &entry.value,
            &format!("`server.security_headers.custom.{name}`"),
        )?;
        validate_header_value(
            &value.value,
            &format!("`server.security_headers.custom.{name}`"),
            &value.span,
        )?;
        custom.insert(name.clone(), value.value);
    }
    Ok(custom)
}

/// Header names are emitted with `HeaderName::from_static`, which panics on uppercase bytes.
fn is_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Header values are emitted with `HeaderValue::from_static`, which panics on any byte outside
/// visible ASCII and tab.
fn validate_header_value(value: &str, context: &str, span: &SourceSpan) -> Result<(), Diagnostic> {
    if value
        .bytes()
        .all(|byte| byte == b'\t' || (32..127).contains(&byte))
    {
        Ok(())
    } else {
        Err(Diagnostic::error(
            "AS1013",
            format!("{context} must contain only visible ASCII characters"),
            span.clone(),
        ))
    }
}

fn invalid_value(context: &str, expected: &str, span: &SourceSpan) -> Diagnostic {
    Diagnostic::error(
        "AS1013",
        format!("{context} must be one of {expected}"),
        span.clone(),
    )
}
