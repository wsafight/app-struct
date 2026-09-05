use super::{IrValidationError, push};
use crate::AppIr;
use std::collections::BTreeSet;

pub(super) fn validate_report(ir: &AppIr, errors: &mut Vec<IrValidationError>) {
    let report = &ir.report;
    if !report.enabled {
        if !report.templates.is_empty() {
            push(
                errors,
                "report.templates",
                "must be empty when report is disabled",
            );
        }
        return;
    }
    if !ir.auth.enabled || !ir.jobs.enabled || !ir.file.enabled {
        push(
            errors,
            "report.enabled",
            "requires auth, jobs, and file modules to be enabled",
        );
    }
    if !ir
        .jobs
        .queues
        .iter()
        .any(|queue| queue.name == report.queue)
    {
        push(
            errors,
            "report.queue",
            "must reference a declared jobs queue",
        );
    }
    if !(1..=4 * 1024 * 1024).contains(&report.max_input_bytes) {
        push(
            errors,
            "report.max_input_bytes",
            "must be between 1 and 4194304",
        );
    }
    if !(1..=3650).contains(&report.retention_days) {
        push(
            errors,
            "report.retention_days",
            "must be between 1 and 3650",
        );
    }
    if report.templates.is_empty() {
        push(
            errors,
            "report.templates",
            "must contain at least one template",
        );
    }
    validate_reader_roles(ir, errors);
    validate_templates(ir, errors);
}

fn validate_reader_roles(ir: &AppIr, errors: &mut Vec<IrValidationError>) {
    let mut seen = BTreeSet::new();
    for (index, role) in ir.report.reader_roles.iter().enumerate() {
        if !ir.auth.roles.contains(role) {
            push(
                errors,
                format!("report.reader_roles[{index}]"),
                format!("unknown auth role `{role}`"),
            );
        }
        if !seen.insert(role) {
            push(
                errors,
                format!("report.reader_roles[{index}]"),
                format!("duplicate role `{role}`"),
            );
        }
    }
}

fn validate_templates(ir: &AppIr, errors: &mut Vec<IrValidationError>) {
    let mut identities = BTreeSet::new();
    for (index, template) in ir.report.templates.iter().enumerate() {
        let path = format!("report.templates[{index}]");
        if template.name.is_empty() || template.name.len() > 80 {
            push(
                errors,
                format!("{path}.name"),
                "must contain between 1 and 80 bytes",
            );
        }
        if template.version == 0 || template.data_schema_version == 0 {
            push(
                errors,
                path.clone(),
                "version fields must be greater than zero",
            );
        }
        if template.document_type != "pdf" {
            push(
                errors,
                format!("{path}.document_type"),
                "only `pdf` is supported",
            );
        }
        if template.body.is_empty() || !template.artifact_digest.starts_with("sha256:") {
            push(
                errors,
                path.clone(),
                "requires a body and sha256 artifact digest",
            );
        }
        if serde_json::from_str::<serde_json::Value>(&template.input_schema)
            .ok()
            .is_none_or(|schema| !schema.is_object())
        {
            push(
                errors,
                format!("{path}.input_schema"),
                "must be a JSON object schema",
            );
        }
        if !identities.insert((&template.name, template.version)) {
            push(
                errors,
                path,
                "duplicates a report template name and version",
            );
        }
    }
}
