use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string, expect_u64,
};
use super::{Located, SurfaceWebhookEndpoint, SurfaceWebhooks};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceWebhooks, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceWebhooks::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("webhooks") else {
        return Ok(SurfaceWebhooks::default());
    };
    let webhooks = expect_mapping(&entry.value, "`modules.webhooks`")?;
    ensure_known_keys(
        webhooks,
        &[
            "enabled",
            "poll_interval_ms",
            "connect_timeout_ms",
            "read_timeout_ms",
            "request_timeout_ms",
            "endpoints",
        ],
        "`modules.webhooks`",
    )?;
    let enabled = webhooks
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.webhooks.enabled`"))
        .transpose()?
        .unwrap_or(true);
    Ok(SurfaceWebhooks {
        enabled,
        poll_interval_ms: webhooks
            .get("poll_interval_ms")
            .map(|entry| expect_u64(&entry.value, "webhook poll interval"))
            .transpose()?,
        connect_timeout_ms: webhooks
            .get("connect_timeout_ms")
            .map(|entry| expect_u64(&entry.value, "webhook connect timeout"))
            .transpose()?,
        read_timeout_ms: webhooks
            .get("read_timeout_ms")
            .map(|entry| expect_u64(&entry.value, "webhook read timeout"))
            .transpose()?,
        request_timeout_ms: webhooks
            .get("request_timeout_ms")
            .map(|entry| expect_u64(&entry.value, "webhook request timeout"))
            .transpose()?,
        endpoints: webhooks
            .get("endpoints")
            .map(decode_endpoints)
            .transpose()?
            .unwrap_or_default(),
        span: Some(entry.value.span.clone()),
    })
}

fn decode_endpoints(entry: &MappingEntry) -> Result<Vec<SurfaceWebhookEndpoint>, Diagnostic> {
    let endpoints = expect_mapping(&entry.value, "`modules.webhooks.endpoints`")?;
    endpoints
        .iter()
        .map(|(name, entry)| {
            let endpoint = expect_mapping(&entry.value, "webhook endpoint")?;
            ensure_known_keys(
                endpoint,
                &[
                    "url",
                    "secret_env",
                    "events",
                    "max_attempts",
                    "backoff_seconds",
                ],
                "webhook endpoint",
            )?;
            let required = |key: &str| {
                endpoint
                    .get(key)
                    .map(|entry| expect_string(&entry.value, &format!("webhook `{key}`")))
                    .transpose()?
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "AS3071",
                            format!("webhook endpoint requires `{key}`"),
                            entry.value.span.clone(),
                        )
                    })
            };
            let events = endpoint
                .get("events")
                .map(|entry| decode_events(&entry.value))
                .transpose()?
                .unwrap_or_default();
            Ok(SurfaceWebhookEndpoint {
                name: Located {
                    value: name.clone(),
                    span: entry.key_span.clone(),
                },
                url: required("url")?,
                secret_env: required("secret_env")?,
                events,
                max_attempts: endpoint
                    .get("max_attempts")
                    .map(|entry| expect_u64(&entry.value, "webhook max attempts"))
                    .transpose()?,
                backoff_seconds: endpoint
                    .get("backoff_seconds")
                    .map(|entry| expect_u64(&entry.value, "webhook backoff"))
                    .transpose()?,
            })
        })
        .collect()
}

fn decode_events(node: &crate::yaml::Node) -> Result<Vec<Located<String>>, Diagnostic> {
    expect_sequence(node, "webhook `events`")?
        .iter()
        .map(|event| expect_string(event, "webhook event"))
        .collect()
}
