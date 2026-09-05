use appstruct_compiler::compile_project;
use appstruct_migrate::extract;
use std::path::Path;

#[test]
fn report_schema_keeps_execution_ownership_in_jobs() {
    let project =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-report-project");
    let schema = extract(&compile_project(&project).unwrap()).unwrap();
    let runs = schema
        .tables
        .iter()
        .find(|table| table.name == "_appstruct_report_runs")
        .unwrap();
    let columns = runs
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    for required in [
        "execution_job_id",
        "template_id",
        "tenant_id",
        "actor_id",
        "idempotency_scope",
        "request_digest",
        "snapshot_ciphertext",
        "snapshot_digest",
        "stage",
        "progress",
        "result_file_id",
        "expires_at",
    ] {
        assert!(columns.contains(&required), "missing {required}");
    }
    for forbidden in [
        "attempts",
        "max_attempts",
        "locked_by",
        "locked_until",
        "run_at",
    ] {
        assert!(
            !columns.contains(&forbidden),
            "ReportRun duplicated Jobs column {forbidden}"
        );
    }
    assert!(schema.unique_constraints.iter().any(|constraint| {
        constraint.table == "_appstruct_report_templates"
            && constraint.columns == ["name", "version"]
    }));
    for target in [
        "_appstruct_jobs",
        "_appstruct_report_templates",
        "_appstruct_files",
    ] {
        assert!(
            schema.foreign_keys.iter().any(|key| {
                key.source_table == "_appstruct_report_runs" && key.target_table == target
            }),
            "missing ReportRun FK to {target}"
        );
    }
    assert_eq!(
        schema
            .indexes
            .iter()
            .filter(|index| index.id.starts_with("appstruct::report::"))
            .count(),
        3,
    );
}

#[test]
fn jobs_status_supports_cancellation() {
    let project =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-report-project");
    let schema = extract(&compile_project(&project).unwrap()).unwrap();
    let jobs = schema
        .tables
        .iter()
        .find(|table| table.name == "_appstruct_jobs")
        .unwrap();
    let status = jobs
        .columns
        .iter()
        .find(|column| column.name == "status")
        .unwrap();
    let appstruct_migrate::DatabaseType::Enum { values } = &status.data_type else {
        panic!("jobs status must be enum");
    };
    assert!(values.iter().any(|value| value == "cancelled"));
}
