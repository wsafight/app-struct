use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string, expect_u64,
    required,
};
use super::{Located, SurfaceReport, SurfaceReportTemplate};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceReport, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceReport::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("report") else {
        return Ok(SurfaceReport::default());
    };
    let report = expect_mapping(&entry.value, "`modules.report`")?;
    ensure_known_keys(
        report,
        &[
            "enabled",
            "renderer",
            "queue",
            "max_input_bytes",
            "retention_days",
            "reader_roles",
            "templates",
        ],
        "`modules.report`",
    )?;
    let enabled = report
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.report.enabled`"))
        .transpose()?
        .unwrap_or(true);
    Ok(SurfaceReport {
        enabled,
        renderer: string(report.get("renderer"), "report renderer")?,
        queue: string(report.get("queue"), "report queue")?,
        max_input_bytes: number(report.get("max_input_bytes"), "report max input bytes")?,
        retention_days: number(report.get("retention_days"), "report retention days")?,
        reader_roles: report
            .get("reader_roles")
            .map(|value| {
                expect_sequence(&value.value, "`modules.report.reader_roles`")?
                    .iter()
                    .map(|item| expect_string(item, "report reader role"))
                    .collect()
            })
            .transpose()?
            .unwrap_or_default(),
        templates: report
            .get("templates")
            .map(decode_templates)
            .transpose()?
            .unwrap_or_default(),
        span: Some(entry.value.span.clone()),
    })
}

fn decode_templates(entry: &MappingEntry) -> Result<Vec<SurfaceReportTemplate>, Diagnostic> {
    let templates = expect_mapping(&entry.value, "`modules.report.templates`")?;
    templates
        .iter()
        .map(|(name, entry)| {
            let template = expect_mapping(&entry.value, "report template")?;
            ensure_known_keys(
                template,
                &["version", "body", "input_schema", "data_schema_version"],
                "report template",
            )?;
            Ok(SurfaceReportTemplate {
                name: Located {
                    value: name.clone(),
                    span: entry.key_span.clone(),
                },
                version: expect_u64(
                    &required(template, "version", &entry.value.span)?.value,
                    "report template version",
                )?,
                body: expect_string(
                    &required(template, "body", &entry.value.span)?.value,
                    "report template body",
                )?,
                input_schema: expect_string(
                    &required(template, "input_schema", &entry.value.span)?.value,
                    "report template input schema",
                )?,
                data_schema_version: number(
                    template.get("data_schema_version"),
                    "report data schema version",
                )?,
            })
        })
        .collect()
}

fn string(
    entry: Option<&MappingEntry>,
    context: &str,
) -> Result<Option<Located<String>>, Diagnostic> {
    entry
        .map(|entry| expect_string(&entry.value, context))
        .transpose()
}

fn number(entry: Option<&MappingEntry>, context: &str) -> Result<Option<Located<u64>>, Diagnostic> {
    entry
        .map(|entry| expect_u64(&entry.value, context))
        .transpose()
}
