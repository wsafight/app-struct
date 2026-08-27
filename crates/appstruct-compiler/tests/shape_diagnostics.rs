use appstruct_compiler::compile_project;
use appstruct_ir::Diagnostic;
use std::fs;

#[test]
fn aggregates_independent_root_shape_errors_in_stable_source_order() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        concat!(
            "version: []\n",
            "app: invalid\n",
            "database: []\n",
            "includes: invalid\n",
            "unknown_first: true\n",
            "unknown_second: true\n",
        ),
    )
    .unwrap();

    let first = compile_project(project.path()).unwrap_err();
    let second = compile_project(project.path()).unwrap_err();
    assert_eq!(signature(&first), signature(&second));
    assert_eq!(first.len(), 6);
    assert_source_ordered(&first);
    assert!(first.iter().any(|diagnostic| {
        diagnostic.code == "AS1007"
            && diagnostic.message == "`version` must be a non-negative integer"
    }));
    assert!(first.iter().any(|diagnostic| {
        diagnostic.code == "AS1007" && diagnostic.message == "`app` must be a mapping"
    }));
    assert!(first.iter().any(|diagnostic| {
        diagnostic.code == "AS1007" && diagnostic.message == "`database` must be a mapping"
    }));
    assert_eq!(
        first
            .iter()
            .filter(|diagnostic| diagnostic.code == "AS1012")
            .count(),
        2
    );
}

#[test]
fn aggregates_shape_errors_across_sibling_domain_definitions() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir(project.path().join("spec")).unwrap();
    fs::write(
        project.path().join("appstruct.yaml"),
        "version: 1\napp: { name: demo }\ndatabase: { provider: postgres }\nincludes: [spec/domain.yaml]\n",
    )
    .unwrap();
    fs::write(
        project.path().join("spec/domain.yaml"),
        concat!(
            "domain: demo\n",
            "entities:\n",
            "  First: []\n",
            "  Second: invalid\n",
            "value_objects:\n",
            "  BrokenValue: []\n",
            "commands:\n",
            "  BrokenCommand: []\n",
            "pages:\n",
            "  BrokenPage: []\n",
        ),
    )
    .unwrap();

    let first = compile_project(project.path()).unwrap_err();
    let second = compile_project(project.path()).unwrap_err();
    assert_eq!(signature(&first), signature(&second));
    assert_eq!(first.len(), 5);
    assert!(first.iter().all(|diagnostic| diagnostic.code == "AS1007"));
    assert_source_ordered(&first);
    assert_eq!(
        first
            .iter()
            .map(|diagnostic| diagnostic.primary.span.line)
            .collect::<Vec<_>>(),
        [3, 4, 6, 8, 10]
    );
}

fn signature(diagnostics: &[Diagnostic]) -> Vec<(&str, &str, usize)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.message.as_str(),
                diagnostic.primary.span.start,
            )
        })
        .collect()
}

fn assert_source_ordered(diagnostics: &[Diagnostic]) {
    assert!(diagnostics.windows(2).all(|pair| {
        let left = &pair[0].primary.span;
        let right = &pair[1].primary.span;
        (left.file.as_str(), left.start) <= (right.file.as_str(), right.start)
    }));
}
