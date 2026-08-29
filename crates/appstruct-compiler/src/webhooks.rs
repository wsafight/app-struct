use crate::surface::{SurfaceWebhookEndpoint, SurfaceWebhooks};
use appstruct_ir::{Diagnostic, SourceSpan, WebhookEndpointIr, WebhooksIr};
use std::collections::BTreeSet;

pub(crate) fn lower_webhooks(
    webhooks: &SurfaceWebhooks,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> WebhooksIr {
    if !webhooks.enabled {
        return WebhooksIr::default();
    }
    let span = webhooks.span.as_ref().unwrap_or(fallback);
    if webhooks.endpoints.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3071",
            "enabled webhooks module requires at least one endpoint",
            span.clone(),
        ));
    }
    let poll_interval_ms = webhooks.poll_interval_ms.as_ref().map_or(250, |value| {
        if !(10..=60_000).contains(&value.value) {
            diagnostics.push(Diagnostic::error(
                "AS3072",
                "webhook `poll_interval_ms` must be between 10 and 60000",
                value.span.clone(),
            ));
        }
        value.value.clamp(10, 60_000)
    });
    let mut names = BTreeSet::new();
    let mut endpoints = webhooks
        .endpoints
        .iter()
        .map(|endpoint| lower_endpoint(endpoint, &mut names, diagnostics))
        .collect::<Vec<_>>();
    endpoints.sort_by(|left, right| left.name.cmp(&right.name));
    WebhooksIr {
        enabled: true,
        poll_interval_ms,
        endpoints,
    }
}

fn lower_endpoint(
    endpoint: &SurfaceWebhookEndpoint,
    names: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> WebhookEndpointIr {
    if !valid_name(&endpoint.name.value) || !names.insert(endpoint.name.value.clone()) {
        diagnostics.push(Diagnostic::error(
            "AS3073",
            "webhook endpoint name must be unique and use lowercase letters, digits, `_`, or `-`",
            endpoint.name.span.clone(),
        ));
    }
    if !valid_url(&endpoint.url.value) {
        diagnostics.push(Diagnostic::error(
            "AS3074",
            "webhook URL must use HTTPS (HTTP is allowed only for localhost)",
            endpoint.url.span.clone(),
        ));
    }
    if !valid_env(&endpoint.secret_env.value) {
        diagnostics.push(Diagnostic::error(
            "AS3075",
            "webhook `secret_env` must be an uppercase environment variable name",
            endpoint.secret_env.span.clone(),
        ));
    }
    if endpoint.events.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3076",
            "webhook endpoint requires at least one event",
            endpoint.name.span.clone(),
        ));
    }
    for event in &endpoint.events {
        if event.value != "*" && !valid_event(&event.value) {
            diagnostics.push(Diagnostic::error(
                "AS3077",
                "webhook event must use lowercase letters, digits, `.`, `_`, or `-`",
                event.span.clone(),
            ));
        }
    }
    WebhookEndpointIr {
        name: endpoint.name.value.clone(),
        url: endpoint.url.value.clone(),
        secret_env: endpoint.secret_env.value.clone(),
        events: endpoint
            .events
            .iter()
            .map(|event| event.value.clone())
            .collect(),
        max_attempts: u32::try_from(ranged(
            endpoint.max_attempts.as_ref(),
            5,
            1,
            100,
            "AS3078",
            diagnostics,
        ))
        .unwrap_or(100),
        backoff_seconds: ranged(
            endpoint.backoff_seconds.as_ref(),
            2,
            1,
            3_600,
            "AS3079",
            diagnostics,
        ),
    }
}

fn ranged(
    value: Option<&crate::surface::Located<u64>>,
    default: u64,
    minimum: u64,
    maximum: u64,
    code: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> u64 {
    value.map_or(default, |value| {
        if !(minimum..=maximum).contains(&value.value) {
            diagnostics.push(Diagnostic::error(
                code,
                format!("value must be between {minimum} and {maximum}"),
                value.span.clone(),
            ));
        }
        value.value.clamp(minimum, maximum)
    })
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
}

fn valid_event(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

fn valid_env(value: &str) -> bool {
    value.starts_with(|c: char| c.is_ascii_uppercase())
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn valid_url(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://localhost:")
        || value.starts_with("http://127.0.0.1:")
}
