use serde::{Deserialize, Serialize};

/// Database settings relevant to deterministic generation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseIr {
    pub provider: DatabaseProvider,
    pub dev_mode: DatabaseDevMode,
    pub dev_migration: DatabaseMigrationPolicy,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_unmanaged_tables: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseProvider {
    Postgres,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseDevMode {
    Managed,
    External,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseMigrationPolicy {
    Auto,
    Prompt,
    Never,
    #[default]
    Unmanaged,
}
