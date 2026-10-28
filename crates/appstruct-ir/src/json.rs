use crate::{AppIr, IR_VERSION};
use serde_json::Value;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum IrJsonError {
    InvalidJson(serde_json::Error),
    MissingVersion,
    UnsupportedVersion { found: u64 },
}

impl fmt::Display for IrJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(formatter, "invalid IR JSON: {error}"),
            Self::MissingVersion => formatter.write_str("IR JSON is missing integer `ir_version`"),
            Self::UnsupportedVersion { found } => write!(
                formatter,
                "unsupported IR version {found}; expected {IR_VERSION}; recompile the App Spec"
            ),
        }
    }
}

impl Error for IrJsonError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            Self::MissingVersion | Self::UnsupportedVersion { .. } => None,
        }
    }
}

impl From<serde_json::Error> for IrJsonError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidJson(error)
    }
}

/// Parse the current IR format without upgrading historical representations.
///
/// # Errors
///
/// Rejects invalid JSON, missing versions, and every version other than `IR_VERSION`.
pub fn from_json(source: &str) -> Result<AppIr, IrJsonError> {
    let value: Value = serde_json::from_str(source)?;
    let version = value
        .get("ir_version")
        .and_then(Value::as_u64)
        .ok_or(IrJsonError::MissingVersion)?;
    if version != u64::from(IR_VERSION) {
        return Err(IrJsonError::UnsupportedVersion { found: version });
    }
    serde_json::from_value(value).map_err(IrJsonError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    const GOLDEN: &str = include_str!("../../../tests/golden/m0-app-ir.json");

    #[test]
    fn current_ir_round_trips_without_migration() {
        let ir = from_json(GOLDEN).unwrap();
        assert_eq!(ir.ir_version, IR_VERSION);
        assert_eq!(from_json(&serde_json::to_string(&ir).unwrap()).unwrap(), ir);
    }

    #[test]
    fn current_format_requires_always_serialized_fields() {
        for field in ["billing", "database"] {
            let mut value: Value = serde_json::from_str(GOLDEN).unwrap();
            value.as_object_mut().unwrap().remove(field);
            assert!(from_json(&value.to_string()).is_err());
        }
        let mut value: Value = serde_json::from_str(GOLDEN).unwrap();
        value["database"]
            .as_object_mut()
            .unwrap()
            .remove("dev_migration");
        assert!(from_json(&value.to_string()).is_err());
    }

    #[test]
    fn rejects_missing_older_and_future_versions() {
        for found in [
            0,
            7,
            11,
            u64::from(IR_VERSION - 1),
            u64::from(IR_VERSION + 1),
            u64::MAX,
        ] {
            let mut value: Value = serde_json::from_str(GOLDEN).unwrap();
            value["ir_version"] = Value::from(found);
            assert!(matches!(
                from_json(&value.to_string()),
                Err(IrJsonError::UnsupportedVersion { .. })
            ));
        }
        assert!(matches!(from_json("{}"), Err(IrJsonError::MissingVersion)));
        let error = from_json("{").unwrap_err();
        assert!(error.source().is_some());
        assert!(error.to_string().contains("invalid IR JSON"));
    }
}
