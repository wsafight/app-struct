use appstruct_compiler::compile_project;
use appstruct_ir::{ChartDimensionIr, ChartKindIr, ChartMeasureIr};
use std::fs;

fn compile_domain(charts: &str) -> Result<appstruct_ir::AppIr, Vec<appstruct_ir::Diagnostic>> {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: chart-test\ndatabase:\n  provider: postgres\nincludes:\n  - spec/domain.yaml\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        format!(
            "domain: core\nentities:\n  Project:\n    fields:\n      id: {{type: uuid, primary_key: true}}\n      status: {{type: enum, values: [active, paused], filterable: true}}\n    access:\n      list: {{public: true}}\n      read: {{public: true}}\n      create: {{public: true}}\n      update: {{public: true}}\n      delete: {{public: true}}\n  Task:\n    fields:\n      id: {{type: uuid, primary_key: true}}\n      project: {{type: relation, target: Project, required: true, filterable: true}}\n      priority: {{type: integer, filterable: true}}\n      title: {{type: string}}\n{charts}    access:\n      list: {{public: true}}\n      read: {{public: true}}\n      create: {{public: true}}\n      update: {{public: true}}\n      delete: {{public: true}}\n"
        ),
    )
    .unwrap();
    compile_project(project.path())
}

#[test]
fn lowers_typed_kpi_and_relation_charts() {
    let ir = compile_domain(
        "    charts:\n      total:\n        label: Total tasks\n        type: kpi\n        measure: count\n      by_project_status:\n        type: horizontal_bar\n        dimension: project.status\n        measure: avg:priority\n        limit: 12\n",
    )
    .unwrap();
    let task = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Task")
        .unwrap();
    assert_eq!(task.views.charts.len(), 2);
    assert_eq!(task.views.charts[0].kind, ChartKindIr::HorizontalBar);
    assert_eq!(task.views.charts[0].label, "By Project Status");
    assert_eq!(task.views.charts[0].limit, 12);
    assert!(matches!(
        &task.views.charts[0].dimension,
        Some(ChartDimensionIr::RelationField { relation, field })
            if relation.0 == "app::Task.project" && field.0 == "app::Project.status"
    ));
    assert!(matches!(
        &task.views.charts[0].measure,
        ChartMeasureIr::Avg { field } if field.0 == "app::Task.priority"
    ));
    assert_eq!(task.views.charts[1].kind, ChartKindIr::Kpi);
    assert!(task.views.charts[1].dimension.is_none());
}

#[test]
fn rejects_unsafe_or_incompatible_chart_shapes() {
    for charts in [
        "    charts:\n      invalid:\n        type: bar\n        dimension: project.owner.status\n        measure: count\n",
        "    charts:\n      invalid:\n        type: donut\n        dimension: project.missing\n        measure: count\n",
        "    charts:\n      invalid:\n        type: kpi\n        dimension: priority\n        measure: count\n",
        "    charts:\n      invalid:\n        type: bar\n        dimension: priority\n        measure: sum:title\n",
    ] {
        let diagnostics = compile_domain(charts).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "AS2044"),
            "{diagnostics:?}"
        );
    }
}
