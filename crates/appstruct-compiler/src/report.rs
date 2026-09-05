use crate::surface::SurfaceReport;
use appstruct_ir::{AuthIr, Diagnostic, FileIr, JobsIr, ReportIr, ReportTemplateIr, SourceSpan};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const DEFAULT_MAX_INPUT_BYTES: u64 = 256 * 1024;
const DEFAULT_RETENTION_DAYS: u32 = 30;

#[allow(clippy::too_many_lines)]
pub(crate) fn lower_report(
    report: &SurfaceReport,
    auth: &AuthIr,
    jobs: &JobsIr,
    file: &FileIr,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> ReportIr {
    if !report.enabled {
        return ReportIr::default();
    }
    let span = report.span.as_ref().unwrap_or(fallback);
    let renderer = match report.renderer.as_ref().map(|value| value.value.as_str()) {
        None | Some("capture") => appstruct_ir::ReportRendererIr::Capture,
        Some("chromium") => appstruct_ir::ReportRendererIr::Chromium,
        Some(_) => {
            diagnostics.push(Diagnostic::error(
                "AS3097",
                "report renderer must be capture or chromium",
                report.renderer.as_ref().unwrap().span.clone(),
            ));
            appstruct_ir::ReportRendererIr::Capture
        }
    };
    if !auth.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3093",
            "enabled report module requires auth",
            span.clone(),
        ));
    }
    if !jobs.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3093",
            "enabled report module requires jobs",
            span.clone(),
        ));
    }
    if !file.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3093",
            "enabled report module requires file storage",
            span.clone(),
        ));
    } else if !file.allowed_content_types.iter().any(|content_type| {
        matches!(
            content_type.as_str(),
            "application/pdf" | "application/*" | "*/*"
        )
    }) {
        diagnostics.push(Diagnostic::error(
            "AS3093",
            "report requires file.allowed_content_types to include `application/pdf`",
            span.clone(),
        ));
    }
    let queue = report
        .queue
        .as_ref()
        .map_or("reports", |value| value.value.as_str());
    if !jobs.queues.iter().any(|candidate| candidate.name == queue) {
        diagnostics.push(Diagnostic::error(
            "AS3094",
            format!("report queue `{queue}` is not declared in modules.jobs.queues"),
            report
                .queue
                .as_ref()
                .map_or_else(|| span.clone(), |value| value.span.clone()),
        ));
    }
    if jobs
        .schedules
        .iter()
        .any(|schedule| schedule.name == "_appstruct_report_retention")
    {
        diagnostics.push(Diagnostic::error(
            "AS3094",
            "job schedule name `_appstruct_report_retention` is reserved by report",
            span.clone(),
        ));
    }
    if report.templates.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3095",
            "enabled report module requires at least one template",
            span.clone(),
        ));
    }
    let roles = auth
        .roles
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut seen_reader_roles = BTreeSet::new();
    for role in &report.reader_roles {
        if !roles.contains(role.value.as_str()) {
            diagnostics.push(Diagnostic::error(
                "AS3096",
                format!("unknown report reader role `{}`", role.value),
                role.span.clone(),
            ));
        }
        if !seen_reader_roles.insert(role.value.as_str()) {
            diagnostics.push(Diagnostic::error(
                "AS3096",
                format!("duplicate report reader role `{}`", role.value),
                role.span.clone(),
            ));
        }
    }
    let templates = report
        .templates
        .iter()
        .filter_map(|template| lower_template(template, diagnostics))
        .collect();
    let max_input_bytes = bounded_u64(
        report.max_input_bytes.as_ref(),
        DEFAULT_MAX_INPUT_BYTES,
        1,
        4 * 1024 * 1024,
        "max_input_bytes",
        diagnostics,
    );
    let retention_days = u32::try_from(bounded_u64(
        report.retention_days.as_ref(),
        u64::from(DEFAULT_RETENTION_DAYS),
        1,
        3650,
        "retention_days",
        diagnostics,
    ))
    .unwrap_or(DEFAULT_RETENTION_DAYS);
    if renderer == appstruct_ir::ReportRendererIr::Chromium && max_input_bytes > 1024 * 1024 {
        diagnostics.push(Diagnostic::error(
            "AS3097",
            "chromium report snapshots are limited to 1 MiB",
            span.clone(),
        ));
    }
    ReportIr {
        enabled: true,
        renderer,
        queue: queue.to_owned(),
        max_input_bytes,
        retention_days,
        reader_roles: report
            .reader_roles
            .iter()
            .map(|role| role.value.clone())
            .collect(),
        templates,
    }
}

fn lower_template(
    template: &crate::surface::SurfaceReportTemplate,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ReportTemplateIr> {
    if !valid_name(&template.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS3097",
            format!("invalid report template name `{}`", template.name.value),
            template.name.span.clone(),
        ));
    }
    if template.version.value == 0 || template.version.value > u64::from(u32::MAX) {
        diagnostics.push(Diagnostic::error(
            "AS3097",
            "report template version must be between 1 and 4294967295",
            template.version.span.clone(),
        ));
    }
    if template
        .data_schema_version
        .as_ref()
        .is_some_and(|value| value.value == 0 || value.value > u64::from(u32::MAX))
    {
        diagnostics.push(Diagnostic::error(
            "AS3097",
            "report template data_schema_version must be between 1 and 4294967295",
            template
                .data_schema_version
                .as_ref()
                .expect("checked above")
                .span
                .clone(),
        ));
    }
    if template.body.value.is_empty() || template.body.value.len() > 1024 * 1024 {
        diagnostics.push(Diagnostic::error(
            "AS3098",
            "report template body must contain between 1 byte and 1 MiB",
            template.body.span.clone(),
        ));
    }
    let schema = match serde_json::from_str::<serde_json::Value>(&template.input_schema.value) {
        Ok(schema) if schema.is_object() && !contains_external_ref(&schema) => schema,
        Ok(_) => {
            diagnostics.push(Diagnostic::error(
                "AS3099",
                "report input_schema must be an object and cannot contain external `$ref` values",
                template.input_schema.span.clone(),
            ));
            return None;
        }
        Err(error) => {
            diagnostics.push(Diagnostic::error(
                "AS3099",
                format!("report input_schema is not valid JSON: {error}"),
                template.input_schema.span.clone(),
            ));
            return None;
        }
    };
    let input_schema = serde_json::to_string(&schema).expect("JSON value serializes");
    Some(ReportTemplateIr {
        name: template.name.value.clone(),
        version: u32::try_from(template.version.value).unwrap_or(u32::MAX),
        document_type: "pdf".to_owned(),
        artifact_digest: format!(
            "sha256:{:x}",
            Sha256::digest(template.body.value.as_bytes())
        ),
        body: template.body.value.clone(),
        input_schema,
        data_schema_version: template
            .data_schema_version
            .as_ref()
            .map_or(1, |value| u32::try_from(value.value).unwrap_or(u32::MAX)),
    })
}

fn contains_external_ref(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            (key == "$ref"
                && value
                    .as_str()
                    .is_some_and(|reference| !reference.starts_with('#')))
                || contains_external_ref(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_external_ref),
        _ => false,
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn bounded_u64(
    value: Option<&crate::surface::Located<u64>>,
    default: u64,
    minimum: u64,
    maximum: u64,
    name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> u64 {
    let Some(value) = value else { return default };
    if !(minimum..=maximum).contains(&value.value) {
        diagnostics.push(Diagnostic::error(
            "AS3097",
            format!("report {name} must be between {minimum} and {maximum}"),
            value.span.clone(),
        ));
    }
    value.value.clamp(minimum, maximum)
}
