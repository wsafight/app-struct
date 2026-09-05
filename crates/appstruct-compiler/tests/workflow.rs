use appstruct_compiler::compile_project;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m7-workflow-project")
}

#[test]
fn lowers_workflow_contract() {
    let ir = compile_project(&fixture()).unwrap();
    let order = ir
        .entities
        .iter()
        .find(|entity| entity.rust_name == "Order")
        .unwrap();
    let workflow = order.workflow.as_ref().unwrap();
    assert_eq!(workflow.initial, "draft");
    assert_eq!(workflow.transitions.len(), 3);
    assert_eq!(workflow.transitions[2].name, "submit");
    assert_eq!(
        workflow.transitions[1].input.as_deref(),
        Some("app::RejectOrderInput")
    );
    assert_eq!(order.workflow_field().unwrap().rust_name, "status");
}

#[test]
fn rejects_invalid_workflow_contracts() {
    let cases = [
        (
            "      status:\n        type: string\n        required: true\n",
            "    workflow:\n      field: status\n      initial: draft\n      transitions:\n        submit: { from: [draft], to: done, access: { public: true } }\n",
            "AS3081",
        ),
        (
            "      status:\n        type: enum\n        values: [draft, done]\n",
            "    workflow:\n      field: status\n      initial: draft\n      transitions:\n        submit: { from: [draft], to: done, access: { public: true } }\n",
            "AS3082",
        ),
        (
            "      status:\n        type: enum\n        required: true\n        values: [draft, done, lost]\n",
            "    workflow:\n      field: status\n      initial: draft\n      transitions:\n        submit: { from: [draft], to: done, access: { public: true } }\n",
            "AS3090",
        ),
    ];
    for (fields, workflow, code) in cases {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("spec")).unwrap();
        fs::write(
            project.path().join("appstruct.yaml"),
            "version: 1\napp:\n  name: workflow-test\ndatabase:\n  provider: postgres\nincludes: [spec/main.yaml]\n",
        )
        .unwrap();
        fs::write(
            project.path().join("spec/main.yaml"),
            format!(
                "domain: test\nentities:\n  Item:\n    fields:\n      id:\n        type: uuid\n        primary_key: true\n        generated: uuid_v7\n{fields}{workflow}    access:\n      list: {{ public: true }}\n      read: {{ public: true }}\n      create: {{ public: true }}\n      update: {{ public: true }}\n      delete: {{ public: true }}\n",
            ),
        )
        .unwrap();
        let diagnostics = compile_project(project.path()).unwrap_err();
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {diagnostics:?}",
        );
    }
}
