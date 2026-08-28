use appstruct_compiler::compile_project;
use appstruct_migrate::{ColumnSchema, DatabaseType, SchemaRisk, diff, extract, lint_plan};
use std::path::Path;

fn schema() -> appstruct_migrate::DatabaseSchema {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    extract(&compile_project(&fixture).unwrap()).unwrap()
}

#[test]
fn lint_reports_destructive_and_non_null_changes_with_stable_codes() {
    let before = schema();
    let mut after = before.clone();
    after.tables[0].columns.push(ColumnSchema {
        id: "app::Project.required_name".to_owned(),
        name: "required_name".to_owned(),
        data_type: DatabaseType::Text,
        nullable: false,
        primary_key: false,
        unique: false,
        default: None,
        generated: None,
    });
    after.tables.pop();

    let plan = diff(&before, &after);
    assert!(
        plan.changes
            .iter()
            .any(|change| change.risk.schema == SchemaRisk::Destructive)
    );
    let issues = lint_plan(&plan);
    assert!(issues.iter().any(|issue| issue.code == "AS4201"));
    assert!(issues.iter().any(|issue| issue.code == "AS4204"));
}
