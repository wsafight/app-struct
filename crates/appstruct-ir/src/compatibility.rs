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
            migrate_v11(&mut value)?;
            migrate_v12(&mut value);
        }
        Ok(8) => {
            migrate_v8(&mut value)?;
            migrate_v9(&mut value)?;
            migrate_v10(&mut value)?;
            migrate_v11(&mut value)?;
            migrate_v12(&mut value);
        }
        Ok(9) => {
            migrate_v9(&mut value)?;
            migrate_v10(&mut value)?;
            migrate_v11(&mut value)?;
            migrate_v12(&mut value);
        }
        Ok(10) => {
            migrate_v10(&mut value)?;
            migrate_v11(&mut value)?;
            migrate_v12(&mut value);
        }
        Ok(11) => {
            migrate_v11(&mut value)?;
            migrate_v12(&mut value);
        }
        Ok(12) => migrate_v12(&mut value),
        Ok(13 | 14) => value["ir_version"] = Value::from(IR_VERSION),
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

fn migrate_v11(value: &mut Value) -> Result<(), IrCompatibilityError> {
    let root = value
        .as_object_mut()
        .ok_or_else(|| invalid_shape("IR v11 root must be an object"))?;
    if !root.contains_key("report") {
        root.insert(
            "report".to_owned(),
            serde_json::to_value(crate::ReportIr::default())?,
        );
    }
    if !root.contains_key("activity") {
        root.insert(
            "activity".to_owned(),
            serde_json::to_value(crate::ActivityIr::default())?,
        );
    }
    value["ir_version"] = Value::from(IR_VERSION);
    Ok(())
}

fn migrate_v12(value: &mut Value) {
    value["ir_version"] = Value::from(IR_VERSION);
}

fn invalid_shape(message: &str) -> IrCompatibilityError {
    IrCompatibilityError::InvalidJson(serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    )))
}

#[cfg(test)]
mod tests;
