use super::MigrationError;
use crate::{ColumnSchema, DatabaseSchema, DatabaseType};
use appstruct_ir::{GeneratedValueIr, OnDeleteIr};
use postgres::Client;
use std::collections::{BTreeMap, BTreeSet};

mod constraints;
mod normalize;

use constraints::{ForeignKeyShape, IndexShape, UniqueConstraintShape};
use normalize::{expected_default, expected_type, sql_literals};

const HISTORY_TABLE: &str = "_appstruct_migrations";

#[derive(Default)]
struct ActualColumn {
    data_type: String,
    nullable: bool,
    default: Option<String>,
    identity: bool,
}

pub(super) fn detect(
    client: &mut Client,
    expected: &DatabaseSchema,
) -> Result<Vec<String>, MigrationError> {
    let actual_tables = tables(client)?;
    let actual_columns = columns(client)?;
    let key_constraints = constraints::key_constraints(client)?;
    let actual_unique_constraints = constraints::unique_constraints(client)?;
    let actual_indexes = constraints::indexes(client)?;
    let actual_foreign_keys = constraints::foreign_keys(client)?;
    let checks = check_constraints(client)?;
    let expected_tables = expected
        .tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut issues = Vec::new();

    for table in &expected.tables {
        if !actual_tables.contains(&table.name) {
            issues.push(format!("missing table `{}`", table.name));
            continue;
        }
        compare_columns(
            table,
            &actual_columns,
            &key_constraints,
            &checks,
            &mut issues,
        );
    }
    for table in actual_tables {
        if table != HISTORY_TABLE && !expected_tables.contains(table.as_str()) {
            issues.push(format!("unexpected table `{table}`"));
        }
    }
    compare_unique_constraints(expected, &actual_unique_constraints, &mut issues);
    compare_indexes(expected, &actual_indexes, &mut issues);
    compare_foreign_keys(expected, &actual_foreign_keys, &mut issues);
    issues.sort();
    issues.dedup();
    Ok(issues)
}

fn compare_indexes(
    expected: &DatabaseSchema,
    actual: &BTreeSet<IndexShape>,
    issues: &mut Vec<String>,
) {
    let expected = expected
        .indexes
        .iter()
        .map(|index| IndexShape {
            table: index.table.clone(),
            columns: index.columns.clone(),
            unique: index.unique,
            predicate: index
                .predicate
                .as_ref()
                .map(|value| value.trim().to_owned()),
        })
        .collect::<BTreeSet<_>>();
    for index in expected.difference(actual) {
        issues.push(format!(
            "missing {}index on `{}` ({}){}",
            if index.unique { "unique " } else { "" },
            index.table,
            index.columns.join(", "),
            index
                .predicate
                .as_ref()
                .map_or_else(String::new, |value| format!(" WHERE ({value})"))
        ));
    }
    for index in actual.difference(&expected) {
        issues.push(format!(
            "unexpected {}index on `{}` ({}){}",
            if index.unique { "unique " } else { "" },
            index.table,
            index.columns.join(", "),
            index
                .predicate
                .as_ref()
                .map_or_else(String::new, |value| format!(" WHERE ({value})"))
        ));
    }
}

fn tables(client: &mut Client) -> Result<BTreeSet<String>, MigrationError> {
    client
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'",
            &[],
        )
        .map_err(|error| database_error("cannot inspect database tables", &error))?
        .into_iter()
        .map(|row| {
            row.try_get("table_name")
                .map_err(|error| database_error("invalid table catalog row", &error))
        })
        .collect()
}

fn columns(
    client: &mut Client,
) -> Result<BTreeMap<(String, String), ActualColumn>, MigrationError> {
    client
        .query(
            r"SELECT table_name, column_name, data_type, is_nullable, column_default, is_identity
FROM information_schema.columns
WHERE table_schema = current_schema()",
            &[],
        )
        .map_err(|error| database_error("cannot inspect database columns", &error))?
        .into_iter()
        .map(|row| {
            let table = catalog_string(&row, "table_name")?;
            let column = catalog_string(&row, "column_name")?;
            let data_type = catalog_string(&row, "data_type")?;
            let nullable = catalog_string(&row, "is_nullable")? == "YES";
            let default = row
                .try_get("column_default")
                .map_err(|error| database_error("invalid column default catalog value", &error))?;
            let identity = catalog_string(&row, "is_identity")? == "YES";
            Ok((
                (table, column),
                ActualColumn {
                    data_type,
                    nullable,
                    default,
                    identity,
                },
            ))
        })
        .collect()
}

fn check_constraints(client: &mut Client) -> Result<BTreeMap<String, Vec<String>>, MigrationError> {
    let rows = client
        .query(
            r"SELECT relation.relname AS table_name, pg_get_constraintdef(con.oid) AS definition
FROM pg_constraint con
JOIN pg_class relation ON relation.oid = con.conrelid
JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
WHERE namespace.nspname = current_schema() AND con.contype = 'c'",
            &[],
        )
        .map_err(|error| database_error("cannot inspect check constraints", &error))?;
    let mut checks = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        checks
            .entry(catalog_string(&row, "table_name")?)
            .or_default()
            .push(catalog_string(&row, "definition")?);
    }
    Ok(checks)
}

fn compare_columns(
    table: &crate::TableSchema,
    actual_columns: &BTreeMap<(String, String), ActualColumn>,
    constraints: &BTreeMap<(String, String), BTreeSet<String>>,
    checks: &BTreeMap<String, Vec<String>>,
    issues: &mut Vec<String>,
) {
    let expected_names = table
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<BTreeSet<_>>();
    for expected in &table.columns {
        let key = (table.name.clone(), expected.name.clone());
        let Some(actual) = actual_columns.get(&key) else {
            issues.push(format!("missing column `{}.{}`", table.name, expected.name));
            continue;
        };
        compare_column(table, expected, actual, constraints.get(&key), issues);
        if let DatabaseType::Enum { values } = &expected.data_type {
            compare_enum_check(&table.name, &expected.name, values, checks, issues);
        }
    }
    for (actual_table, actual_column) in actual_columns.keys() {
        if actual_table == &table.name && !expected_names.contains(actual_column.as_str()) {
            issues.push(format!(
                "unexpected column `{actual_table}.{actual_column}`"
            ));
        }
    }
}

fn compare_column(
    table: &crate::TableSchema,
    expected: &ColumnSchema,
    actual: &ActualColumn,
    constraints: Option<&BTreeSet<String>>,
    issues: &mut Vec<String>,
) {
    let name = format!("{}.{}", table.name, expected.name);
    if actual.data_type != expected_type(&expected.data_type) {
        issues.push(format!(
            "column `{name}` has type `{}`, expected `{}`",
            actual.data_type,
            expected_type(&expected.data_type)
        ));
    }
    if actual.nullable != expected.nullable {
        issues.push(format!("column `{name}` has different nullability"));
    }
    let primary_key = constraints.is_some_and(|values| values.contains("PRIMARY KEY"));
    let unique = constraints.is_some_and(|values| values.contains("UNIQUE"));
    if primary_key != expected.primary_key {
        issues.push(format!("column `{name}` has different primary-key status"));
    }
    if !expected.primary_key && unique != expected.unique {
        issues.push(format!("column `{name}` has different unique status"));
    }
    let identity = matches!(expected.generated, Some(GeneratedValueIr::AutoIncrement));
    if actual.identity != identity {
        issues.push(format!("column `{name}` has different identity status"));
    }
    if normalize::default(actual.default.as_deref()) != expected_default(expected) {
        issues.push(format!("column `{name}` has a different default"));
    }
}

fn compare_enum_check(
    table: &str,
    column: &str,
    expected: &[String],
    checks: &BTreeMap<String, Vec<String>>,
    issues: &mut Vec<String>,
) {
    let expected = expected.iter().cloned().collect::<BTreeSet<_>>();
    let matching = checks.get(table).and_then(|definitions| {
        definitions
            .iter()
            .find(|definition| definition.contains(column))
    });
    let actual = matching.map(|definition| sql_literals(definition));
    if actual.as_ref() != Some(&expected) {
        issues.push(format!(
            "enum check for `{table}.{column}` differs from the snapshot"
        ));
    }
}

fn compare_foreign_keys(
    expected: &DatabaseSchema,
    actual: &BTreeSet<ForeignKeyShape>,
    issues: &mut Vec<String>,
) {
    let expected = expected
        .foreign_keys
        .iter()
        .map(|foreign_key| ForeignKeyShape {
            source_table: foreign_key.source_table.clone(),
            source_columns: foreign_key.source_columns.clone(),
            target_table: foreign_key.target_table.clone(),
            target_columns: foreign_key.target_columns.clone(),
            on_delete: match foreign_key.on_delete {
                OnDeleteIr::Restrict => "RESTRICT",
                OnDeleteIr::Cascade => "CASCADE",
                OnDeleteIr::SetNull => "SET NULL",
            }
            .to_owned(),
        })
        .collect::<BTreeSet<_>>();
    for foreign_key in expected.difference(actual) {
        issues.push(format!(
            "missing foreign key `{}({})` -> `{}({})`",
            foreign_key.source_table,
            foreign_key.source_columns.join(", "),
            foreign_key.target_table,
            foreign_key.target_columns.join(", ")
        ));
    }
    for foreign_key in actual.difference(&expected) {
        issues.push(format!(
            "unexpected foreign key `{}({})` -> `{}({})`",
            foreign_key.source_table,
            foreign_key.source_columns.join(", "),
            foreign_key.target_table,
            foreign_key.target_columns.join(", ")
        ));
    }
}

fn compare_unique_constraints(
    expected: &DatabaseSchema,
    actual: &BTreeSet<UniqueConstraintShape>,
    issues: &mut Vec<String>,
) {
    let expected = expected
        .unique_constraints
        .iter()
        .map(|constraint| UniqueConstraintShape {
            table: constraint.table.clone(),
            columns: constraint.columns.clone(),
        })
        .collect::<BTreeSet<_>>();
    for constraint in expected.difference(actual) {
        issues.push(format!(
            "missing unique constraint `{}({})`",
            constraint.table,
            constraint.columns.join(", ")
        ));
    }
    for constraint in actual.difference(&expected) {
        issues.push(format!(
            "unexpected unique constraint `{}({})`",
            constraint.table,
            constraint.columns.join(", ")
        ));
    }
}

fn catalog_string(row: &postgres::Row, name: &str) -> Result<String, MigrationError> {
    row.try_get(name)
        .map_err(|error| database_error("invalid PostgreSQL catalog row", &error))
}

fn database_error(context: &str, error: &postgres::Error) -> MigrationError {
    MigrationError::Database(format!("{context}: {}", super::database_message(error)))
}
