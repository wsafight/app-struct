use appstruct_compiler::compile_project;
use appstruct_migrate::extract;
use std::path::Path;

#[test]
fn activity_schema_preserves_tombstones_and_resource_scoping() {
    let project =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-activity-project");
    let schema = extract(&compile_project(&project).unwrap()).unwrap();
    let entries = schema
        .tables
        .iter()
        .find(|table| table.name == "_appstruct_activity_entries")
        .unwrap();
    let columns = entries
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    for required in [
        "resource",
        "record_id",
        "tenant_id",
        "actor_id",
        "kind",
        "body",
        "event",
        "payload",
        "attachment_file_id",
        "withdrawn_at",
        "withdrawn_by",
        "governance_reason",
        "occurred_at",
    ] {
        assert!(columns.contains(&required), "missing {required}");
    }
    assert!(schema.foreign_keys.iter().any(|key| {
        key.source_table == "_appstruct_activity_entries" && key.target_table == "_appstruct_files"
    }));
    assert_eq!(
        schema
            .indexes
            .iter()
            .filter(|index| index.id.starts_with("appstruct::activity::"))
            .count(),
        3,
    );
    assert!(schema.indexes.iter().any(|index| {
        index.id == "appstruct::activity::record_timeline"
            && index.columns == ["tenant_id", "resource", "record_id", "occurred_at", "id"]
    }));
}
