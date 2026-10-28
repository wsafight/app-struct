//! Database schema contracts and deterministic SQL rendering for `AppStruct`.

mod schema;
pub mod sql;

pub use schema::{
    ColumnSchema, DatabaseSchema, DatabaseType, ForeignKeySchema, IndexSchema, SCHEMA_VERSION,
    SeedSchema, SeedValueSchema, TableSchema, UniqueConstraintSchema, extract, from_json, to_json,
};
pub use sql::initial_migration;
