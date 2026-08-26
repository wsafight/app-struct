use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn generation_plan_is_byte_deterministic() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m2-project");
    let first = plan(&compile_project(&fixture).unwrap()).unwrap();
    let second = plan(&compile_project(&fixture).unwrap()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn compiler_and_generator_meet_mvp_performance_budgets() {
    let ten = synthetic_project(10);
    let started = Instant::now();
    let ten_ir = compile_project(ten.path()).unwrap();
    assert_within("10 entity IR compilation", started.elapsed(), 500);
    let ten_artifacts = plan(&ten_ir).unwrap();
    assert!(!ten_artifacts.is_empty());
    assert_within("10 entity compile and generation", started.elapsed(), 1_000);

    let hundred = synthetic_project(100);
    let started = Instant::now();
    let hundred_ir = compile_project(hundred.path()).unwrap();
    let hundred_artifacts = plan(&hundred_ir).unwrap();
    assert!(!hundred_artifacts.is_empty());
    assert_within(
        "100 entity compile and generation",
        started.elapsed(),
        10_000,
    );
}

fn synthetic_project(entity_count: usize) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("spec")).unwrap();
    fs::write(
        root.path().join("appstruct.yaml"),
        "version: 1\napp:\n  name: quality-gate\ndatabase:\n  provider: postgres\n  dev:\n    mode: external\nincludes:\n  - spec/main.yaml\n",
    )
    .unwrap();
    let mut spec = String::from("domain: quality\nentities:\n");
    for index in 0..entity_count {
        write!(
            spec,
            "  Entity{index:03}:\n    table: entities_{index:03}\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n        generated: uuid_v7\n      name:\n        type: string\n        required: true\n        max_length: 120\n        searchable: true\n        sortable: true\n      created_at:\n        type: datetime\n        generated: now\n        sortable: true\n    access:\n      list: {{ public: true }}\n      read: {{ public: true }}\n      create: {{ public: true }}\n      update: {{ public: true }}\n      delete: {{ public: true }}\n"
        )
        .unwrap();
    }
    fs::write(root.path().join("spec/main.yaml"), spec).unwrap();
    root
}

fn assert_within(operation: &str, elapsed: Duration, budget_ms: u128) {
    eprintln!(
        "{operation}: {} ms (budget < {budget_ms} ms)",
        elapsed.as_millis()
    );
    assert!(
        elapsed.as_millis() < budget_ms,
        "{operation} took {} ms; budget is < {budget_ms} ms",
        elapsed.as_millis()
    );
}
