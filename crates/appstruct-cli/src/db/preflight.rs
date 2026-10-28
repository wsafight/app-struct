use super::render::Draft;
use appstruct_ir::Diagnostic;
use appstruct_migrate::{DriftStatus, IntrospectedSchema};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Serialize)]
pub(super) struct Preflight {
    status: &'static str,
    include: IncludeProposal,
    omitted_tables: Vec<String>,
    missing_revision: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    structure_issues: Vec<String>,
    history_checked: bool,
}

#[derive(Serialize)]
struct IncludeProposal {
    file: &'static str,
    action: &'static str,
    value: String,
    requires_confirmation: bool,
}

pub(super) fn build(
    project: &Path,
    output: &Path,
    draft: &Draft,
    original: &IntrospectedSchema,
    database_url: &str,
) -> Preflight {
    let imported = draft.entity_tables.values().collect::<BTreeSet<_>>();
    let omitted_tables = original
        .tables
        .iter()
        .filter(|table| !imported.contains(&table.name))
        .map(|table| table.name.clone())
        .collect::<Vec<_>>();
    let missing_revision = original
        .tables
        .iter()
        .filter(|table| imported.contains(&table.name))
        .filter(|table| !table.columns.iter().any(|column| column.name == "revision"))
        .map(|table| table.name.clone())
        .collect::<Vec<_>>();
    let mut result = Preflight {
        status: "blocked",
        include: IncludeProposal {
            file: "appstruct.yaml",
            action: "ensure_include",
            value: output.to_string_lossy().into_owned(),
            requires_confirmation: true,
        },
        omitted_tables,
        missing_revision,
        diagnostics: Vec::new(),
        structure_issues: Vec::new(),
        history_checked: false,
    };
    let report =
        match appstruct_compiler::compile_project_with_domain(project, output, &draft.source) {
            Ok(report) => report,
            Err(diagnostics) => {
                result.diagnostics = diagnostics;
                return result;
            }
        };
    result.diagnostics = report.diagnostics;
    let expected = match appstruct_migrate::extract(&report.ir) {
        Ok(expected) => expected,
        Err(error) => {
            result.structure_issues.push(error.to_string());
            return result;
        }
    };
    if !expected.seeds.is_empty() {
        result.structure_issues.push(
            "Baseline does not accept seeds; review seed data separately after adoption."
                .to_owned(),
        );
    }
    match appstruct_migrate::inspect_schema_structure(database_url, &original.name, &expected) {
        Ok(DriftStatus::Clean) => {}
        Ok(DriftStatus::Detected(issues)) => result.structure_issues.extend(issues),
        Ok(DriftStatus::Deferred) => unreachable!("structure inspection has no migration history"),
        Err(error) => result.structure_issues.push(error.to_string()),
    }
    if result.structure_issues.is_empty()
        && result.omitted_tables.is_empty()
        && result.missing_revision.is_empty()
    {
        result.status = "structure_matches";
    }
    result
}

pub(super) fn print(report: &Preflight) {
    println!(
        "Adoption preflight: {} (read-only; migration history not checked)",
        report.status
    );
    println!(
        "  Proposed configuration: ensure `{}` is in appstruct.yaml includes; review before applying",
        report.include.value
    );
    for table in &report.omitted_tables {
        println!("  Not imported: `{table}`; strict baseline does not ignore extra tables");
    }
    for table in &report.missing_revision {
        println!(
            "  Missing concurrency column: `{table}.revision`; review the required structure change"
        );
    }
    for diagnostic in &report.diagnostics {
        println!("  {}: {}", diagnostic.code, diagnostic.message);
    }
    for issue in &report.structure_issues {
        println!("  Structure: {issue}");
    }
}
