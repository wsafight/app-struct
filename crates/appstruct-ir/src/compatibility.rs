use crate::{AppIr, IR_VERSION};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt;

const MIN_COMPATIBLE_IR_VERSION: u32 = appstruct_contracts::IR.minimum;

#[derive(Debug)]
pub enum IrCompatibilityError {
    InvalidJson(serde_json::Error),
    MissingVersion,
    UnsupportedVersion { found: u64 },
    LegacyModuleGraph,
}

impl fmt::Display for IrCompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(formatter, "invalid IR JSON: {error}"),
            Self::MissingVersion => formatter.write_str("IR JSON is missing integer `ir_version`"),
            Self::UnsupportedVersion { found } => write!(
                formatter,
                "unsupported IR version {found}; supported versions are {MIN_COMPATIBLE_IR_VERSION} through {IR_VERSION}"
            ),
            Self::LegacyModuleGraph => formatter.write_str(
                "IR v7 with resolved modules cannot be migrated safely; recompile the App Spec",
            ),
        }
    }
}

impl Error for IrCompatibilityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            Self::MissingVersion | Self::UnsupportedVersion { .. } | Self::LegacyModuleGraph => {
                None
            }
        }
    }
}

impl From<serde_json::Error> for IrCompatibilityError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidJson(error)
    }
}

/// Parse current IR or migrate a supported legacy representation in memory.
///
/// # Errors
///
/// Returns an explicit compatibility error for missing, future, or semantically unsafe legacy
/// versions. Callers should recompile the source App Spec when migration is unsafe.
pub fn from_compatible_json(source: &str) -> Result<AppIr, IrCompatibilityError> {
    let mut value: Value = serde_json::from_str(source)?;
    let version = value
        .get("ir_version")
        .and_then(Value::as_u64)
        .ok_or(IrCompatibilityError::MissingVersion)?;
    match u32::try_from(version) {
        Ok(IR_VERSION) => {}
        Ok(7) => {
            migrate_v7(&mut value)?;
            migrate_v10(&mut value)?;
        }
        Ok(8) => {
            migrate_v8(&mut value)?;
            migrate_v9(&mut value)?;
            migrate_v10(&mut value)?;
        }
        Ok(9) => {
            migrate_v9(&mut value)?;
            migrate_v10(&mut value)?;
        }
        Ok(10) => migrate_v10(&mut value)?,
        _ => return Err(IrCompatibilityError::UnsupportedVersion { found: version }),
    }
    serde_json::from_value(value).map_err(IrCompatibilityError::from)
}

fn migrate_v7(value: &mut Value) -> Result<(), IrCompatibilityError> {
    let modules = value
        .get("modules")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_shape("IR v7 is missing array `modules`"))?;
    if !modules.is_empty() {
        return Err(IrCompatibilityError::LegacyModuleGraph);
    }
    value["ir_version"] = Value::from(IR_VERSION);
    Ok(())
}

fn migrate_v8(value: &mut Value) -> Result<(), IrCompatibilityError> {
    let modules = value
        .get_mut("modules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid_shape("IR v8 is missing array `modules`"))?;
    for module in modules {
        let module = module
            .as_object_mut()
            .ok_or_else(|| invalid_shape("IR v8 `modules` entries must be objects"))?;
        module.insert("origin".to_owned(), Value::String("official".to_owned()));
        module.insert("artifacts".to_owned(), Value::Array(Vec::new()));
    }
    value["ir_version"] = Value::from(9);
    Ok(())
}

fn migrate_v9(value: &mut Value) -> Result<(), IrCompatibilityError> {
    let modules = value
        .get_mut("modules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid_shape("IR v9 is missing array `modules`"))?;
    for module in modules {
        let module = module
            .as_object_mut()
            .ok_or_else(|| invalid_shape("IR v9 `modules` entries must be objects"))?;
        module.insert("manifest_path".to_owned(), Value::Null);
        module.insert("content_sha256".to_owned(), Value::Null);
        let artifacts = module
            .get_mut("artifacts")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid_shape("IR v9 module is missing array `artifacts`"))?;
        for artifact in artifacts {
            let artifact = artifact
                .as_object_mut()
                .ok_or_else(|| invalid_shape("IR v9 `artifacts` entries must be objects"))?;
            let content = artifact
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_shape("IR v9 artifact is missing string `content`"))?;
            let byte_len = u64::try_from(content.len())
                .map_err(|_| invalid_shape("IR v9 artifact byte count overflowed"))?;
            let sha256 = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
            artifact.insert("source".to_owned(), Value::Null);
            artifact.insert("sha256".to_owned(), Value::String(sha256));
            artifact.insert("byte_len".to_owned(), Value::from(byte_len));
        }
    }
    value["ir_version"] = Value::from(IR_VERSION);
    Ok(())
}

fn migrate_v10(value: &mut Value) -> Result<(), IrCompatibilityError> {
    let database = value
        .get_mut("database")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_shape("IR v10 is missing object `database`"))?;
    database.insert(
        "dev_migration".to_owned(),
        Value::String("unmanaged".to_owned()),
    );
    value["ir_version"] = Value::from(IR_VERSION);
    Ok(())
}

fn invalid_shape(message: &str) -> IrCompatibilityError {
    IrCompatibilityError::InvalidJson(serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    )))
}

#[cfg(test)]
mod tests {
    use super::{IrCompatibilityError, from_compatible_json};
    use crate::IR_VERSION;

    #[test]
    fn migrates_module_free_v7_ir() {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
        value["ir_version"] = 7.into();
        value["modules"] = serde_json::json!([]);
        let migrated = from_compatible_json(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(migrated.ir_version, IR_VERSION);
        assert!(migrated.modules.is_empty());
    }

    #[test]
    fn rejects_legacy_module_graph_and_future_versions() {
        let source = include_str!("../../../tests/golden/m0-app-ir.json");
        let legacy = source.replacen(
            &format!("\"ir_version\": {IR_VERSION}"),
            "\"ir_version\": 7",
            1,
        );
        assert!(matches!(
            from_compatible_json(&legacy),
            Err(IrCompatibilityError::LegacyModuleGraph)
        ));

        let future = source.replacen(
            &format!("\"ir_version\": {IR_VERSION}"),
            "\"ir_version\": 999",
            1,
        );
        assert!(matches!(
            from_compatible_json(&future),
            Err(IrCompatibilityError::UnsupportedVersion { found: 999 })
        ));
    }

    #[test]
    fn migrates_v8_official_modules() {
        let source = include_str!("../../../tests/golden/m0-app-ir.json");
        let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
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
        let source = include_str!("../../../tests/golden/m0-app-ir.json");
        let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
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
        let source = include_str!("../../../tests/golden/m0-app-ir.json");
        let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
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
}
