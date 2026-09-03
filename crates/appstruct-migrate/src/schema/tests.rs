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
fn json_round_trip_accepts_compatible_versions() {
    let json = to_json(&empty()).unwrap();
    let parsed = from_json(&json).unwrap();
    assert_eq!(parsed.schema_version, SCHEMA_VERSION);

    let mut legacy = empty();
    legacy.schema_version = MIN_COMPATIBLE_SCHEMA_VERSION;
    let parsed = from_json(&to_json(&legacy).unwrap()).unwrap();
    assert_eq!(parsed.schema_version, SCHEMA_VERSION);

    let mut future = empty();
    future.schema_version = SCHEMA_VERSION + 10;
    assert!(from_json(&to_json(&future).unwrap()).is_err());
    assert!(from_json("{").is_err());
}

#[test]
fn extract_rejects_invalid_ir() {
    let mut ir: AppIr =
        serde_json::from_str(include_str!("../../../../tests/golden/m0-app-ir.json")).unwrap();
    ir.ir_version = 0;
    assert!(extract(&ir).is_err());
}
