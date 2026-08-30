use crate::{MigrationError, connect_database};
use postgres::Client;
use std::collections::BTreeMap;

mod catalog;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrospectedSchema {
    pub name: String,
    pub tables: Vec<IntrospectedTable>,
    pub foreign_keys: Vec<IntrospectedForeignKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrospectedTable {
    pub name: String,
    pub columns: Vec<IntrospectedColumn>,
    pub primary_key: Vec<String>,
    pub unique_constraints: Vec<Vec<String>>,
    pub indexes: Vec<IntrospectedIndex>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrospectedIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub predicate: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrospectedColumn {
    pub name: String,
    pub data_type: String,
    pub udt_schema: String,
    pub udt_name: String,
    pub domain_schema: Option<String>,
    pub domain_name: Option<String>,
    pub nullable: bool,
    pub default: Option<String>,
    pub identity: bool,
    pub generated: bool,
    pub max_length: Option<i32>,
    pub enum_values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrospectedForeignKey {
    pub name: String,
    pub source_table: String,
    pub source_columns: Vec<String>,
    pub target_schema: String,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub on_delete: String,
}

/// Inspect one PostgreSQL schema without changing database state.
///
/// # Errors
///
/// Returns an error when the connection fails, the schema does not exist, or catalog rows cannot
/// be read.
pub fn inspect_database_schema(
    database_url: &str,
    schema: &str,
) -> Result<IntrospectedSchema, MigrationError> {
    let mut client = connect_database(database_url)?;
    inspect(&mut client, schema)
}

fn inspect(client: &mut Client, schema: &str) -> Result<IntrospectedSchema, MigrationError> {
    if !catalog::schema_exists(client, schema)? {
        return Err(MigrationError::Database(format!(
            "PostgreSQL schema `{schema}` does not exist"
        )));
    }
    let table_names = catalog::tables(client, schema)?;
    let mut columns = catalog::columns(client, schema)?;
    let enum_values = catalog::enum_values(client)?;
    for (_, column) in &mut columns {
        column.enum_values = enum_values
            .get(&(column.udt_schema.clone(), column.udt_name.clone()))
            .cloned()
            .unwrap_or_default();
    }
    let constraints = catalog::key_constraints(client, schema)?;
    let indexes = catalog::indexes(client, schema)?;
    let mut columns_by_table = BTreeMap::<String, Vec<_>>::new();
    for column in columns {
        columns_by_table.entry(column.0).or_default().push(column.1);
    }
    let mut primary_keys = BTreeMap::<String, Vec<String>>::new();
    let mut unique_constraints = BTreeMap::<String, Vec<Vec<String>>>::new();
    for constraint in constraints {
        if constraint.primary {
            primary_keys.insert(constraint.table, constraint.columns);
        } else {
            unique_constraints
                .entry(constraint.table)
                .or_default()
                .push(constraint.columns);
        }
    }
    let mut indexes_by_table = BTreeMap::<String, Vec<IntrospectedIndex>>::new();
    for index in indexes {
        indexes_by_table
            .entry(index.table.clone())
            .or_default()
            .push(IntrospectedIndex {
                name: index.name,
                columns: index.columns,
                unique: index.unique,
                predicate: index.predicate,
            });
    }
    let tables = table_names
        .into_iter()
        .filter(|name| !name.starts_with("_appstruct_"))
        .map(|name| IntrospectedTable {
            columns: columns_by_table.remove(&name).unwrap_or_default(),
            primary_key: primary_keys.remove(&name).unwrap_or_default(),
            unique_constraints: unique_constraints.remove(&name).unwrap_or_default(),
            indexes: indexes_by_table.remove(&name).unwrap_or_default(),
            name,
        })
        .collect();
    Ok(IntrospectedSchema {
        name: schema.to_owned(),
        tables,
        foreign_keys: catalog::foreign_keys(client, schema)?
            .into_iter()
            .filter(|key| !key.source_table.starts_with("_appstruct_"))
            .collect(),
    })
}
