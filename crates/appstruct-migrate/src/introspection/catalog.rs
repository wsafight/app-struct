use super::{IntrospectedColumn, IntrospectedForeignKey};
use crate::MigrationError;
use postgres::{Client, Row};
use std::collections::BTreeMap;

pub(super) struct KeyConstraint {
    pub table: String,
    pub primary: bool,
    pub columns: Vec<String>,
}

pub(super) fn schema_exists(client: &mut Client, schema: &str) -> Result<bool, MigrationError> {
    client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_namespace WHERE nspname = $1) AS present",
            &[&schema],
        )
        .and_then(|row| row.try_get("present"))
        .map_err(|error| database_error("cannot inspect PostgreSQL schemas", &error))
}

pub(super) fn tables(client: &mut Client, schema: &str) -> Result<Vec<String>, MigrationError> {
    client
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = $1 AND table_type = 'BASE TABLE' ORDER BY table_name",
            &[&schema],
        )
        .map_err(|error| database_error("cannot inspect database tables", &error))?
        .into_iter()
        .map(|row| text(&row, "table_name"))
        .collect()
}

pub(super) fn columns(
    client: &mut Client,
    schema: &str,
) -> Result<Vec<(String, IntrospectedColumn)>, MigrationError> {
    client
        .query(
            r"SELECT table_name, column_name, data_type, udt_schema, udt_name, is_nullable,
       column_default, is_identity, is_generated, character_maximum_length
FROM information_schema.columns
WHERE table_schema = $1
ORDER BY table_name, ordinal_position",
            &[&schema],
        )
        .map_err(|error| database_error("cannot inspect database columns", &error))?
        .into_iter()
        .map(|row| {
            Ok((
                text(&row, "table_name")?,
                IntrospectedColumn {
                    name: text(&row, "column_name")?,
                    data_type: text(&row, "data_type")?,
                    udt_schema: text(&row, "udt_schema")?,
                    udt_name: text(&row, "udt_name")?,
                    nullable: text(&row, "is_nullable")? == "YES",
                    default: optional_text(&row, "column_default")?,
                    identity: text(&row, "is_identity")? == "YES",
                    generated: text(&row, "is_generated")? != "NEVER",
                    max_length: row.try_get("character_maximum_length").map_err(|error| {
                        database_error("invalid column length catalog value", &error)
                    })?,
                    enum_values: Vec::new(),
                },
            ))
        })
        .collect()
}

pub(super) fn key_constraints(
    client: &mut Client,
    schema: &str,
) -> Result<Vec<KeyConstraint>, MigrationError> {
    let rows = client
        .query(
            r"SELECT relation.relname AS table_name, con.conname AS constraint_name,
       con.contype::text AS constraint_type, attribute.attname AS column_name,
       key.position::bigint AS position
FROM pg_constraint con
JOIN pg_class relation ON relation.oid = con.conrelid
JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS key(number, position) ON true
JOIN pg_attribute attribute
  ON attribute.attrelid = relation.oid AND attribute.attnum = key.number
WHERE namespace.nspname = $1 AND con.contype IN ('p', 'u')
ORDER BY relation.relname, con.conname, key.position",
            &[&schema],
        )
        .map_err(|error| database_error("cannot inspect key constraints", &error))?;
    let mut grouped = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        let key = (
            text(&row, "table_name")?,
            text(&row, "constraint_name")?,
            text(&row, "constraint_type")? == "p",
        );
        let position = row
            .try_get::<_, i64>("position")
            .map_err(|error| database_error("invalid key position", &error))?;
        grouped
            .entry(key)
            .or_default()
            .push((position, text(&row, "column_name")?));
    }
    Ok(grouped
        .into_iter()
        .map(|((table, _, primary), mut columns)| {
            columns.sort_by_key(|(position, _)| *position);
            KeyConstraint {
                table,
                primary,
                columns: columns.into_iter().map(|(_, column)| column).collect(),
            }
        })
        .collect())
}

pub(super) fn foreign_keys(
    client: &mut Client,
    schema: &str,
) -> Result<Vec<IntrospectedForeignKey>, MigrationError> {
    let rows = client
        .query(
            r"SELECT source.relname AS source_table, con.conname AS constraint_name,
       source_attribute.attname AS source_column, target_namespace.nspname AS target_schema,
       target.relname AS target_table, target_attribute.attname AS target_column,
       CASE con.confdeltype WHEN 'r' THEN 'restrict' WHEN 'c' THEN 'cascade'
         WHEN 'n' THEN 'set_null' WHEN 'd' THEN 'set_default' ELSE 'no_action' END AS delete_rule,
       key.position::bigint AS position
FROM pg_constraint con
JOIN pg_class source ON source.oid = con.conrelid
JOIN pg_namespace source_namespace ON source_namespace.oid = source.relnamespace
JOIN pg_class target ON target.oid = con.confrelid
JOIN pg_namespace target_namespace ON target_namespace.oid = target.relnamespace
JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY
  AS key(source_number, target_number, position) ON true
JOIN pg_attribute source_attribute
  ON source_attribute.attrelid = source.oid AND source_attribute.attnum = key.source_number
JOIN pg_attribute target_attribute
  ON target_attribute.attrelid = target.oid AND target_attribute.attnum = key.target_number
WHERE source_namespace.nspname = $1 AND con.contype = 'f'
ORDER BY source.relname, con.conname, key.position",
            &[&schema],
        )
        .map_err(|error| database_error("cannot inspect foreign keys", &error))?;
    group_foreign_keys(rows)
}

fn group_foreign_keys(rows: Vec<Row>) -> Result<Vec<IntrospectedForeignKey>, MigrationError> {
    let mut grouped = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        let key = (
            text(&row, "source_table")?,
            text(&row, "constraint_name")?,
            text(&row, "target_schema")?,
            text(&row, "target_table")?,
            text(&row, "delete_rule")?,
        );
        let position = row
            .try_get::<_, i64>("position")
            .map_err(|error| database_error("invalid foreign-key position", &error))?;
        grouped.entry(key).or_default().push((
            position,
            text(&row, "source_column")?,
            text(&row, "target_column")?,
        ));
    }
    Ok(grouped
        .into_iter()
        .map(
            |((source_table, name, target_schema, target_table, on_delete), mut columns)| {
                columns.sort_by_key(|(position, _, _)| *position);
                IntrospectedForeignKey {
                    name,
                    source_table,
                    source_columns: columns
                        .iter()
                        .map(|(_, source, _)| source.clone())
                        .collect(),
                    target_schema,
                    target_table,
                    target_columns: columns.into_iter().map(|(_, _, target)| target).collect(),
                    on_delete,
                }
            },
        )
        .collect())
}

pub(super) fn enum_values(
    client: &mut Client,
) -> Result<BTreeMap<(String, String), Vec<String>>, MigrationError> {
    let rows = client
        .query(
            r"SELECT namespace.nspname AS type_schema, kind.typname AS type_name, value.enumlabel
FROM pg_type kind
JOIN pg_namespace namespace ON namespace.oid = kind.typnamespace
JOIN pg_enum value ON value.enumtypid = kind.oid
ORDER BY namespace.nspname, kind.typname, value.enumsortorder",
            &[],
        )
        .map_err(|error| database_error("cannot inspect PostgreSQL enum types", &error))?;
    let mut values = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        values
            .entry((text(&row, "type_schema")?, text(&row, "type_name")?))
            .or_default()
            .push(text(&row, "enumlabel")?);
    }
    Ok(values)
}

fn text(row: &Row, name: &str) -> Result<String, MigrationError> {
    row.try_get(name)
        .map_err(|error| database_error("invalid PostgreSQL catalog row", &error))
}

fn optional_text(row: &Row, name: &str) -> Result<Option<String>, MigrationError> {
    row.try_get(name)
        .map_err(|error| database_error("invalid PostgreSQL catalog row", &error))
}

fn database_error(context: &str, error: &postgres::Error) -> MigrationError {
    MigrationError::Database(format!("{context}: {error}"))
}
