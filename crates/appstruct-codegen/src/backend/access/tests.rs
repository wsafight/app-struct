#[allow(dead_code)]
#[path = "../../../tests/support/mod.rs"]
mod support;

use appstruct_ir::{AccessRuleIr, FieldTypeIr};
use quote::{format_ident, quote};
use std::{fs, path::Path};

#[test]
#[allow(clippy::too_many_lines)]
fn generated_scopes_match_policy_truth_tables() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let mut ir = appstruct_compiler::compile_project(&fixture).unwrap();
    let entity = ir
        .entities
        .iter_mut()
        .find(|entity| entity.rust_name == "Project")
        .unwrap();
    let mut deleted = entity
        .fields
        .iter()
        .find(|field| field.rust_name == "created_at")
        .unwrap()
        .clone();
    deleted.id.0 = deleted.id.0.replace("created_at", "deleted_at");
    deleted.rust_name = "deleted_at".to_owned();
    deleted.api_name = "deleted_at".to_owned();
    deleted.column_name = "deleted_at".to_owned();
    deleted.generated = None;
    deleted.nullable = true;
    entity.fields.push(deleted);
    entity.fields.sort_by(|left, right| left.id.cmp(&right.id));
    entity.views.soft_delete = true;
    let owner = entity
        .fields
        .iter()
        .find(|field| matches!(field.ty, FieldTypeIr::Relation { .. }))
        .unwrap()
        .id
        .clone();
    let leaves = vec![
        AccessRuleIr::Public,
        AccessRuleIr::Authenticated,
        AccessRuleIr::Role {
            role: "admin".to_owned(),
        },
        AccessRuleIr::Owner { field: owner },
    ];
    let mut rules = leaves.clone();
    for left in &leaves {
        for right in &leaves {
            rules.push(AccessRuleIr::Any {
                rules: vec![left.clone(), right.clone()],
            });
            rules.push(AccessRuleIr::All {
                rules: vec![left.clone(), right.clone()],
            });
        }
        rules.push(AccessRuleIr::All {
            rules: vec![
                left.clone(),
                AccessRuleIr::Any {
                    rules: leaves.clone(),
                },
            ],
        });
        rules.push(AccessRuleIr::Any {
            rules: vec![
                left.clone(),
                AccessRuleIr::All {
                    rules: leaves.clone(),
                },
            ],
        });
    }
    let mut arms = Vec::new();
    let module = format_ident!("project");
    for (index, rule) in rules.iter().enumerate() {
        let list = super::scope(entity, &module, rule).unwrap();
        let member = super::member_scope(entity, &module, rule).unwrap();
        let trash = super::trash_scope(entity, &module, rule).unwrap();
        let related = super::related_scope(entity, &module, rule).unwrap();
        arms.push(quote! {
            (#index, 0) => { #list }
            (#index, 1) => { #member select = select.filter(access_condition); }
            (#index, 2) => { #trash }
            (#index, 3) => { let mut relation_select = select; #related select = relation_select; }
        });
    }
    let rules_json = serde_json::to_string(&rules).unwrap();
    let cases = quote! {
        fn select_case(index: usize, mode: u8, context: &RequestContext<'_>) -> Result<sea_orm::Select<project::Entity>, ApiError> {
            let mut select = project::Entity::find();
            match (index, mode) { #(#arms)* _ => unreachable!() }
            Ok(select)
        }
        fn rules() -> serde_json::Value { serde_json::from_str(#rules_json).unwrap() }
    };
    let root = tempfile::tempdir().unwrap();
    for artifact in crate::plan(&ir).unwrap() {
        let destination = root.path().join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, artifact.content).unwrap();
    }
    fs::create_dir(root.path().join("backend/tests")).unwrap();
    fs::write(
        root.path().join("backend/tests/access_scopes.rs"),
        format!("{}\n{cases}", include_str!("tests/runtime.rs")),
    )
    .unwrap();
    let output = support::cargo_test(&root.path().join("backend/Cargo.toml"), "access_scopes");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
