use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

const DOMAIN: &str = "domain: preview\nentities:\n  PreviewItem:\n    fields:\n      id: { type: uuid, primary_key: true }\n    access:\n      list: { public: true }\n      read: { public: true }\n      create: { public: true }\n      update: { public: true }\n      delete: { public: true }\n";

#[test]
fn proposed_domain_compiles_in_memory_and_keeps_configuration_and_files_unchanged() {
    let root = fixture();
    let before = std::fs::read(root.join("appstruct.yaml")).unwrap();
    let original = appstruct_compiler::compile_project(&root).unwrap();
    let report = appstruct_compiler::compile_project_with_domain(
        &root,
        Path::new("spec/preview-only.yaml"),
        DOMAIN,
    )
    .unwrap();
    assert_eq!(report.ir.entities.len(), original.entities.len() + 1);
    assert_eq!(before, std::fs::read(root.join("appstruct.yaml")).unwrap());
    assert!(!root.join("spec/preview-only.yaml").exists());
}

#[test]
fn proposed_domains_keep_normal_validation_and_reject_unsafe_paths() {
    for relative in ["../outside.yaml", "/tmp/outside.yaml", "spec/preview.json"] {
        assert!(
            appstruct_compiler::compile_project_with_domain(
                &fixture(),
                Path::new(relative),
                DOMAIN
            )
            .is_err()
        );
    }
    let conflicting = DOMAIN
        .replace("PreviewItem", "Project")
        .replace("domain: preview", "domain: project");
    assert!(
        appstruct_compiler::compile_project_with_domain(
            &fixture(),
            Path::new("spec/conflicting.yaml"),
            &conflicting
        )
        .is_err()
    );
}
