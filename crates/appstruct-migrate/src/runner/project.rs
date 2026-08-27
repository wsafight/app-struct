use super::{MigrationError, SCHEMA_CHECKSUM_PREFIX, TRANSACTION_PREFIX, checksum, history};
use crate::{DatabaseSchema, SCHEMA_VERSION, from_json};
use appstruct_ir::DatabaseProvider;
use std::fs;
use std::path::Path;

pub(super) struct ProjectMigrations {
    pub files: Vec<MigrationFile>,
    pub schema: DatabaseSchema,
}

impl ProjectMigrations {
    pub(super) fn load(project: &Path) -> Result<Self, MigrationError> {
        let files = load_files(&project.join("migrations"))?;
        let snapshot_path = project.join(".appstruct/schema.snapshot.json");
        let snapshot = match fs::read_to_string(&snapshot_path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && files.is_empty() => {
                return Ok(Self {
                    files,
                    schema: empty_schema(),
                });
            }
            Err(error) => {
                return Err(MigrationError::Project(format!(
                    "cannot read `{}`: {error}",
                    snapshot_path.display()
                )));
            }
        };
        let schema = from_json(&snapshot).map_err(|error| {
            MigrationError::Project(format!(
                "invalid schema snapshot `{}`: {error}",
                snapshot_path.display()
            ))
        })?;
        let Some(latest) = files.last() else {
            return if schema.tables.is_empty()
                && schema.unique_constraints.is_empty()
                && schema.foreign_keys.is_empty()
            {
                Ok(Self { files, schema })
            } else {
                Err(MigrationError::Project(
                    "schema snapshot has no corresponding migration file".to_owned(),
                ))
            };
        };
        let expected_checksum = checksum(snapshot.as_bytes());
        if latest.schema_checksum.as_deref() != Some(expected_checksum.as_str()) {
            return Err(MigrationError::Integrity(format!(
                "latest migration `{}` does not match the schema snapshot checksum",
                latest.id
            )));
        }
        Ok(Self { files, schema })
    }
}

pub(super) struct MigrationFile {
    pub id: String,
    pub checksum: String,
    pub sql: String,
    pub transactional: bool,
    pub(super) schema_checksum: Option<String>,
}

fn load_files(directory: &Path) -> Result<Vec<MigrationFile>, MigrationError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(MigrationError::Project(format!(
                "cannot read migration directory `{}`: {error}",
                directory.display()
            )));
        }
    };
    let mut paths = entries
        .map(|entry| {
            entry.map(|value| value.path()).map_err(|error| {
                MigrationError::Project(format!(
                    "cannot inspect migration directory `{}`: {error}",
                    directory.display()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| path.is_file() && path.extension().is_some_and(|value| value == "sql"));
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let id = path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| valid_migration_id(name))
                .ok_or_else(|| {
                    MigrationError::Project(format!(
                        "invalid migration filename `{}`",
                        path.display()
                    ))
                })?
                .to_owned();
            let sql = fs::read_to_string(&path).map_err(|error| {
                MigrationError::Project(format!(
                    "cannot read migration `{}`: {error}",
                    path.display()
                ))
            })?;
            let (transactional, schema_checksum) = directives(&id, &sql)?;
            Ok(MigrationFile {
                id,
                checksum: checksum(sql.as_bytes()),
                sql,
                transactional,
                schema_checksum,
            })
        })
        .collect()
}

fn valid_migration_id(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension == "sql")
        && name
            .strip_suffix(".sql")
            .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(is_id_byte))
}

const fn is_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

pub(super) fn directives(id: &str, sql: &str) -> Result<(bool, Option<String>), MigrationError> {
    let mut transactional = true;
    let mut schema_checksum = None;
    for line in sql.lines().take(16).map(str::trim) {
        if let Some(value) = line.strip_prefix(TRANSACTION_PREFIX) {
            transactional = match value {
                "on" => true,
                "off" => false,
                _ => {
                    return Err(MigrationError::Project(format!(
                        "migration `{id}` has invalid transaction directive `{value}`"
                    )));
                }
            };
        }
        if let Some(value) = line.strip_prefix(SCHEMA_CHECKSUM_PREFIX) {
            if schema_checksum.is_some() || !valid_checksum(value) {
                return Err(MigrationError::Project(format!(
                    "migration `{id}` has an invalid schema checksum directive"
                )));
            }
            schema_checksum = Some(value.to_owned());
        }
    }
    Ok((transactional, schema_checksum))
}

fn valid_checksum(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) struct Reconciliation {
    pub applied: usize,
    pub pending: usize,
}

pub(super) fn reconcile(
    files: &[MigrationFile],
    applied: &[history::HistoryEntry],
) -> Result<Reconciliation, MigrationError> {
    if applied.len() > files.len() {
        return Err(MigrationError::Integrity(
            "database history contains migrations missing from disk".to_owned(),
        ));
    }
    for (index, entry) in applied.iter().enumerate() {
        let Some(file) = files.get(index) else {
            return Err(MigrationError::Integrity(format!(
                "applied migration `{}` is missing from disk",
                entry.id
            )));
        };
        if entry.id != file.id {
            return Err(MigrationError::Integrity(format!(
                "migration history is out of order: expected `{}`, found `{}`",
                file.id, entry.id
            )));
        }
        if entry.state != "applied" {
            return Err(MigrationError::Integrity(format!(
                "migration `{}` is in `{}` state and requires manual recovery",
                entry.id, entry.state
            )));
        }
        if entry.checksum != file.checksum {
            return Err(MigrationError::Integrity(format!(
                "applied migration `{}` was modified after execution",
                entry.id
            )));
        }
    }
    Ok(Reconciliation {
        applied: applied.len(),
        pending: files.len() - applied.len(),
    })
}

pub(super) fn empty_schema() -> DatabaseSchema {
    DatabaseSchema {
        schema_version: SCHEMA_VERSION,
        provider: DatabaseProvider::Postgres,
        tables: Vec::new(),
        unique_constraints: Vec::new(),
        foreign_keys: Vec::new(),
    }
}
