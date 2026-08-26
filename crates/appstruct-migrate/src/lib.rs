//! Database schema extraction, diffing, risk classification, and SQL planning.

mod diff;
mod runner;
mod schema;
mod sql;

pub use diff::{
    ChangeRisk, ExecutionRisk, MigrationPlan, PlannedChange, SchemaChange, SchemaRisk, diff,
};
pub use runner::{
    ApplyReport, DriftStatus, MigrationError, MigrationStatus, apply_project,
    stamp_schema_checksum, status_project,
};
pub use schema::{
    ColumnSchema, DatabaseSchema, DatabaseType, ForeignKeySchema, TableSchema, extract, from_json,
    to_json,
};
pub use sql::{initial_migration, migration_sql};
