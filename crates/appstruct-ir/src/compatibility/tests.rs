use super::{IrCompatibilityError, from_compatible_json};
use crate::IR_VERSION;

const GOLDEN_IR: &str = include_str!("../../../../tests/golden/m0-app-ir.json");

#[test]
fn migrates_module_free_v7_ir() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 7.into();
    value["modules"] = serde_json::json!([]);
    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(migrated.ir_version, IR_VERSION);
    assert!(migrated.modules.is_empty());
}

#[test]
fn rejects_legacy_module_graph_and_future_versions() {
    let legacy = GOLDEN_IR.replacen(
        &format!("\"ir_version\": {IR_VERSION}"),
        "\"ir_version\": 7",
        1,
    );
    assert!(matches!(
        from_compatible_json(&legacy),
        Err(IrCompatibilityError::LegacyModuleGraph)
    ));

    let future = GOLDEN_IR.replacen(
        &format!("\"ir_version\": {IR_VERSION}"),
        "\"ir_version\": 999",
        1,
    );
    assert!(matches!(
        from_compatible_json(&future),
        Err(IrCompatibilityError::UnsupportedVersion { found: 999 })
    ));
    assert!(matches!(
        from_compatible_json("{"),
        Err(IrCompatibilityError::InvalidJson(_))
    ));
    assert!(matches!(
        from_compatible_json("{}"),
        Err(IrCompatibilityError::MissingVersion)
    ));
    assert!(
        from_compatible_json("{")
            .unwrap_err()
            .to_string()
            .contains("invalid IR JSON")
    );
    assert!(
        IrCompatibilityError::MissingVersion
            .to_string()
            .contains("ir_version")
    );
    assert!(
        IrCompatibilityError::UnsupportedVersion { found: 3 }
            .to_string()
            .contains("unsupported IR version")
    );
    assert!(
        IrCompatibilityError::LegacyModuleGraph
            .to_string()
            .contains("recompile")
    );
}

#[test]
fn migrates_v8_official_modules() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 8.into();
    for module in value["modules"].as_array_mut().unwrap() {
        module.as_object_mut().unwrap().remove("origin");
        module.as_object_mut().unwrap().remove("artifacts");
    }

    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(migrated.ir_version, IR_VERSION);
    assert!(migrated.modules.iter().all(|module| {
        module.origin == crate::ModuleOrigin::Official
            && module.manifest_path.is_none()
            && module.content_sha256.is_none()
            && module.artifacts.is_empty()
    }));
}

#[test]
fn migrates_v9_artifact_integrity_without_inventing_source_provenance() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 9.into();
    let module = value["modules"][0].as_object_mut().unwrap();
    module.remove("manifest_path");
    module.remove("content_sha256");
    module.insert(
        "artifacts".to_owned(),
        serde_json::json!([{"path": "README.md", "content": "legacy\n"}]),
    );
    for module in value["modules"].as_array_mut().unwrap().iter_mut().skip(1) {
        module.as_object_mut().unwrap().remove("manifest_path");
        module.as_object_mut().unwrap().remove("content_sha256");
    }

    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();
    let artifact = &migrated.modules[0].artifacts[0];
    assert_eq!(artifact.source, None);
    assert_eq!(artifact.byte_len, 7);
    assert_eq!(
        artifact.sha256,
        "sha256:777d90290a75129dbd33a5ac590f4633ec91d0eb381213761c728004961c3320"
    );
}

#[test]
fn migrates_v10_database_policy_without_enabling_writes() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 10.into();
    value["database"]
        .as_object_mut()
        .unwrap()
        .remove("dev_migration");

    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(migrated.ir_version, IR_VERSION);
    assert_eq!(
        migrated.database.dev_migration,
        crate::DatabaseMigrationPolicy::Unmanaged
    );
}

#[test]
fn migrates_v11_without_enabling_report_or_activity_modules() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 11.into();
    value.as_object_mut().unwrap().remove("report");
    value.as_object_mut().unwrap().remove("activity");
    let module_count = value["modules"].as_array().unwrap().len();

    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();

    assert_eq!(migrated.ir_version, IR_VERSION);
    assert!(!migrated.report.enabled);
    assert!(!migrated.activity.enabled);
    assert_eq!(migrated.modules.len(), module_count);
    assert!(
        migrated
            .modules
            .iter()
            .all(|module| module.name != "appstruct/report" && module.name != "appstruct/activity")
    );
}

#[test]
fn migrates_v12_without_inventing_ui_semantics() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 12.into();
    for entity in value["entities"].as_array_mut().unwrap() {
        for field in entity["fields"].as_array_mut().unwrap() {
            field.as_object_mut().unwrap().remove("ui_semantic");
        }
    }

    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();

    assert_eq!(migrated.ir_version, IR_VERSION);
    assert!(
        migrated
            .entities
            .iter()
            .flat_map(|entity| &entity.fields)
            .all(|field| field.ui_semantic.is_none())
    );
}

#[test]
fn migrating_v11_preserves_explicit_service_contracts() {
    let mut value: serde_json::Value = serde_json::from_str(GOLDEN_IR).unwrap();
    value["ir_version"] = 11.into();
    value["report"] = serde_json::json!({
        "enabled": false,
        "queue": "legacy-reports",
        "max_input_bytes": 42,
        "retention_days": 7,
        "reader_roles": [],
        "templates": []
    });
    value["activity"] = serde_json::json!({
        "enabled": false,
        "max_comment_bytes": 2048,
        "attachments": false,
        "admin_roles": [],
        "resources": []
    });

    let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();

    assert_eq!(migrated.report.queue, "legacy-reports");
    assert_eq!(migrated.report.max_input_bytes, 42);
    assert_eq!(migrated.activity.max_comment_bytes, 2048);
}
