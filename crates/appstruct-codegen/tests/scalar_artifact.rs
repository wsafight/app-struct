#[allow(dead_code)]
mod support;

use appstruct_codegen::plan;
use appstruct_compiler::compile_project;
use std::fs;

#[test]
fn generated_scalar_contracts_preserve_values_and_patch_presence() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("appstruct.yaml"), "version: 1\napp:\n  name: scalar-check\ndatabase:\n  provider: postgres\nincludes: [spec/main.yaml]\n").unwrap();
    fs::create_dir(root.path().join("spec")).unwrap();
    fs::write(
        root.path().join("spec/main.yaml"),
        r"
domain: scalar
entities:
  Sample:
    fields:
      id: {type: uuid, primary_key: true, generated: uuid_v7}
      value: {type: bigint, required: true, filterable: true}
      optional: {type: bigint}
      fallback: {type: bigint, default: 9}
      amount: {type: decimal, filterable: true}
    access:
      list: {public: true}
      read: {public: true}
      create: {public: true}
      update: {public: true}
      delete: {public: true}
value_objects:
  ScalarData:
    fields:
      value: {type: bigint, required: true}
",
    )
    .unwrap();
    let artifacts = plan(&compile_project(root.path()).unwrap()).unwrap();
    for artifact in artifacts {
        let destination = root.path().join("generated").join(artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, artifact.content).unwrap();
    }
    let generated = root.path().join("generated");
    let openapi: serde_json::Value =
        serde_json::from_slice(&fs::read(generated.join("openapi/openapi.json")).unwrap()).unwrap();
    assert_eq!(
        openapi["components"]["schemas"]["Sample"]["properties"]["value"]["type"],
        "string"
    );
    assert_eq!(
        openapi["components"]["schemas"]["Sample"]["properties"]["revision"]["type"],
        "integer"
    );
    let client = fs::read_to_string(generated.join("web/src/generated/client.ts")).unwrap();
    assert!(client.contains("value: string"));
    fs::create_dir(generated.join("backend/tests")).unwrap();
    fs::write(generated.join("backend/tests/scalar_values.rs"), r#"
use appstruct_generated_backend::{api::sample::{CreateInput, UpdateInput}, entities::sample::Model, extensions::ScalarData};
use serde_json::json;

#[test]
fn lossless_values() {
    let value = "9223372036854775807";
    let input: CreateInput = serde_json::from_value(json!({"value": value})).unwrap();
    assert_eq!(input.value, i64::MAX);
    assert!(input.optional.is_none());
    assert!(input.fallback.is_none());
    let absent: UpdateInput = serde_json::from_value(json!({})).unwrap();
    let clear: UpdateInput = serde_json::from_value(json!({"optional": null})).unwrap();
    let set: UpdateInput = serde_json::from_value(json!({"optional": value})).unwrap();
    assert_eq!(absent.optional, None);
    assert_eq!(clear.optional, Some(None));
    assert_eq!(set.optional, Some(Some(i64::MAX)));
    let model: Model = serde_json::from_value(json!({"id": "00000000-0000-4000-8000-000000000001", "value": value, "optional": value, "fallback": "9", "amount": "9007199254740993.15", "revision": 1})).unwrap();
    let json = serde_json::to_value(model).unwrap();
    assert_eq!(json["value"], value);
    assert_eq!(json["optional"], value);
    assert_eq!(json["amount"], "9007199254740993.15");
    assert_eq!(json["revision"], 1);
    assert_eq!(serde_json::to_value(ScalarData { value: i64::MIN }).unwrap()["value"], i64::MIN.to_string());
}
"#).unwrap();
    let output = support::cargo_test(&generated.join("backend/Cargo.toml"), "scalar_values");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
