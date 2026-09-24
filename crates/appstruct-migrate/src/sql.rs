use crate::{MigrationPlan, SchemaChange};
use appstruct_schema::ColumnSchema;
use appstruct_schema::sql::{
    add_foreign_key, add_index, add_seed, add_unique_constraint, column_definition,
    generated_header, quote_ident, sql_default,
};

pub use appstruct_schema::initial_migration;

#[cfg(test)]
use appstruct_ir::{GeneratedValueIr, OnDeleteIr};
#[cfg(test)]
use appstruct_schema::sql::quote_literal;
#[cfg(test)]
use appstruct_schema::{
    DatabaseSchema, DatabaseType, ForeignKeySchema, TableSchema, UniqueConstraintSchema,
};

/// Render SQL for an automatically applicable migration plan.
///
/// # Errors
///
/// Returns an error when the plan contains a blocked or unsupported change.
pub fn migration_sql(plan: &MigrationPlan) -> Result<String, String> {
    if plan.is_blocked() {
        return Err("migration contains changes that require review or input".to_owned());
    }
    let mut statements = vec![generated_header()];
    for planned in &plan.changes {
        statements.push(match &planned.change {
            SchemaChange::AddTable { table } => appstruct_schema::sql::create_table(table),
            SchemaChange::AddColumn { table, column } => format!(
                "ALTER TABLE {} ADD COLUMN {};\n",
                quote_ident(table),
                column_definition(column, true)
            ),
            SchemaChange::AlterColumn {
                table,
                before,
                after,
            } => alter_column(table, before, after)?,
            SchemaChange::AddUniqueConstraint { constraint } => add_unique_constraint(constraint),
            SchemaChange::AddIndex { index } => add_index(index),
            SchemaChange::AddSeed { seed } => add_seed(seed),
            SchemaChange::AddForeignKey { foreign_key } => add_foreign_key(foreign_key),
            SchemaChange::RemoveTable { .. }
            | SchemaChange::RenameTable { .. }
            | SchemaChange::RemoveColumn { .. }
            | SchemaChange::RemoveUniqueConstraint { .. }
            | SchemaChange::RemoveIndex { .. }
            | SchemaChange::RemoveSeed { .. }
            | SchemaChange::RemoveForeignKey { .. } => {
                return Err("destructive migration cannot be rendered automatically".to_owned());
            }
        });
    }
    Ok(format!("{}\n", statements.join("\n")))
}

fn alter_column(
    table: &str,
    before: &ColumnSchema,
    after: &ColumnSchema,
) -> Result<String, String> {
    if before.name != after.name
        || before.data_type != after.data_type
        || before.primary_key != after.primary_key
        || before.unique != after.unique
        || before.generated != after.generated
    {
        return Err("column shape change requires manual migration".to_owned());
    }
    if before.nullable && !after.nullable {
        return Err("adding a not-null constraint requires manual migration".to_owned());
    }
    let prefix = format!(
        "ALTER TABLE {} ALTER COLUMN {}",
        quote_ident(table),
        quote_ident(&after.name)
    );
    let mut statements = Vec::new();
    if before.nullable != after.nullable {
        statements.push(format!("{prefix} DROP NOT NULL;\n"));
    }
    if before.default != after.default {
        statements.push(sql_default(after).map_or_else(
            || format!("{prefix} DROP DEFAULT;\n"),
            |default| format!("{prefix} SET DEFAULT {default};\n"),
        ));
    }
    Ok(statements.concat())
}

#[cfg(test)]
mod tests;
