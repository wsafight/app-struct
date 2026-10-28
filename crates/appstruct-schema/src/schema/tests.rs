use super::*;
use appstruct_ir::DatabaseProvider;

fn empty() -> DatabaseSchema {
    DatabaseSchema {
        schema_version: SCHEMA_VERSION,
        provider: DatabaseProvider::Postgres,
        tables: Vec::new(),
        unique_constraints: Vec::new(),
        indexes: Vec::new(),
        seeds: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

#[test]
fn json_round_trip_rejects_every_noncurrent_version() {
    let json = to_json(&empty()).unwrap();
    let parsed = from_json(&json).unwrap();
    assert_eq!(parsed.schema_version, SCHEMA_VERSION);

    for version in 0..SCHEMA_VERSION {
        let mut old = empty();
        old.schema_version = version;
        assert!(from_json(&to_json(&old).unwrap()).is_err());
    }

    let mut future = empty();
    future.schema_version = SCHEMA_VERSION + 10;
    assert!(from_json(&to_json(&future).unwrap()).is_err());
    assert!(from_json("{").is_err());
    let mut incomplete = serde_json::to_value(empty()).unwrap();
    incomplete
        .as_object_mut()
        .unwrap()
        .remove("unique_constraints");
    assert!(from_json(&incomplete.to_string()).is_err());
}

#[test]
fn extract_rejects_invalid_ir() {
    let mut ir: AppIr =
        serde_json::from_str(include_str!("../../../../tests/golden/m0-app-ir.json")).unwrap();
    ir.ir_version = 0;
    assert!(extract(&ir).is_err());
}
