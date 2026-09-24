//! Database schema diffing, risk classification, and PostgreSQL migration execution.

mod diff;
mod introspection;
mod lint;
mod runner;
mod sql;

pub use appstruct_schema::{
    ColumnSchema, DatabaseSchema, DatabaseType, ForeignKeySchema, IndexSchema,
    MIN_COMPATIBLE_SCHEMA_VERSION, SCHEMA_VERSION, SeedSchema, SeedValueSchema, TableSchema,
    UniqueConstraintSchema, extract, from_json, to_json,
};
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
pub use sql::{initial_migration, migration_sql};
