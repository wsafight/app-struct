use super::{DatabaseSchema, MIN_COMPATIBLE_SCHEMA_VERSION, SCHEMA_VERSION};

/// Serialize a canonical schema snapshot.
///
/// # Errors
///
/// Returns an error if JSON serialization unexpectedly fails.
pub fn to_json(schema: &DatabaseSchema) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_string_pretty(schema)?;
    value.push('\n');
    Ok(value)
}

/// Parse a schema snapshot.
///
/// # Errors
///
/// Returns an error if the snapshot is invalid or incompatible JSON.
pub fn from_json(source: &str) -> Result<DatabaseSchema, serde_json::Error> {
    let mut schema: DatabaseSchema = serde_json::from_str(source)?;
    match schema.schema_version {
        SCHEMA_VERSION => Ok(schema),
        found if (MIN_COMPATIBLE_SCHEMA_VERSION..SCHEMA_VERSION).contains(&found) => {
            schema.schema_version = SCHEMA_VERSION;
            Ok(schema)
        }
        found => Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "unsupported schema snapshot version {found}; supported versions are {MIN_COMPATIBLE_SCHEMA_VERSION} through {SCHEMA_VERSION}"
            ),
        ))),
    }
}
