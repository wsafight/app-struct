//! Database schema extraction, diffing, risk classification, and SQL planning.

mod diff;
mod introspection;
mod lint;
mod runner;
mod schema;
mod sql;

pub use diff::{
    ChangeRisk, ExecutionRisk, MigrationPlan, PlannedChange, SchemaChange, SchemaRisk, diff,
};
pub use introspection::{
    IntrospectedColumn, IntrospectedForeignKey, IntrospectedIndex, IntrospectedSchema,
    IntrospectedTable, inspect_database_schema,
};
pub use lint::{LintSeverity, MigrationLint, lint_plan};
pub use runner::{
    ApplyReport, DriftStatus, MigrationError, MigrationStatus, apply_project, connect_database,
    stamp_schema_checksum, status_project,
};
pub use schema::{
    ColumnSchema, DatabaseSchema, DatabaseType, ForeignKeySchema, IndexSchema,
    MIN_COMPATIBLE_SCHEMA_VERSION, SCHEMA_VERSION, SeedSchema, SeedValueSchema, TableSchema,
    UniqueConstraintSchema, extract, from_json, to_json,
};
pub use sql::{initial_migration, migration_sql};
