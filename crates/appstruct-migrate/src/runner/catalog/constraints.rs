use super::{catalog_string, database_error};
use crate::runner::MigrationError;
use postgres::Client;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ForeignKeyShape {
    pub source_table: String,
    pub source_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub on_delete: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct UniqueConstraintShape {
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct IndexShape {
    pub table: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub predicate: Option<String>,
}

pub(super) fn key_constraints(
    client: &mut Client,
) -> Result<BTreeMap<(String, String), BTreeSet<String>>, MigrationError> {
    let rows = client
        .query(
            r"SELECT tc.table_name, kcu.column_name, tc.constraint_type
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu
  ON kcu.constraint_schema = tc.constraint_schema
 AND kcu.constraint_name = tc.constraint_name
JOIN (
  SELECT constraint_schema, constraint_name, COUNT(*) AS column_count
  FROM information_schema.key_column_usage
  GROUP BY constraint_schema, constraint_name
) sizes
  ON sizes.constraint_schema = tc.constraint_schema
 AND sizes.constraint_name = tc.constraint_name
WHERE tc.table_schema = current_schema()
  AND (tc.constraint_type = 'PRIMARY KEY'
       OR (tc.constraint_type = 'UNIQUE' AND sizes.column_count = 1))",
            &[],
        )
        .map_err(|error| database_error("cannot inspect key constraints", &error))?;
    let mut constraints = BTreeMap::<_, BTreeSet<_>>::new();
    for row in rows {
        let key = (
            catalog_string(&row, "table_name")?,
            catalog_string(&row, "column_name")?,
        );
        constraints
            .entry(key)
            .or_default()
            .insert(catalog_string(&row, "constraint_type")?);
    }
    Ok(constraints)
}

pub(super) fn foreign_keys(
    client: &mut Client,
) -> Result<BTreeSet<ForeignKeyShape>, MigrationError> {
    let rows = client
        .query(
            r"SELECT source.relname AS source_table,
       con.conname AS constraint_name,
       source_attribute.attname AS source_column,
       target.relname AS target_table,
       target_attribute.attname AS target_column,
       CASE con.confdeltype
         WHEN 'r' THEN 'RESTRICT'
         WHEN 'c' THEN 'CASCADE'
         WHEN 'n' THEN 'SET NULL'
         WHEN 'd' THEN 'SET DEFAULT'
         ELSE 'NO ACTION'
       END AS delete_rule,
       key.position::bigint AS position
FROM pg_constraint con
JOIN pg_class source ON source.oid = con.conrelid
JOIN pg_namespace namespace ON namespace.oid = source.relnamespace
JOIN pg_class target ON target.oid = con.confrelid
JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY
  AS key(source_number, target_number, position) ON true
JOIN pg_attribute source_attribute
  ON source_attribute.attrelid = source.oid
 AND source_attribute.attnum = key.source_number
JOIN pg_attribute target_attribute
  ON target_attribute.attrelid = target.oid
 AND target_attribute.attnum = key.target_number
WHERE namespace.nspname = current_schema() AND con.contype = 'f'
ORDER BY source.relname, con.conname, key.position",
            &[],
        )
        .map_err(|error| database_error("cannot inspect foreign keys", &error))?;
    let mut grouped = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        let key = (
            catalog_string(&row, "source_table")?,
            catalog_string(&row, "constraint_name")?,
            catalog_string(&row, "target_table")?,
            catalog_string(&row, "delete_rule")?,
        );
        let position = row
            .try_get::<_, i64>("position")
            .map_err(|error| database_error("invalid foreign-key position", &error))?;
        grouped.entry(key).or_default().push((
            position,
            catalog_string(&row, "source_column")?,
            catalog_string(&row, "target_column")?,
        ));
    }
    Ok(grouped
        .into_iter()
        .map(
            |((source_table, _, target_table, on_delete), mut columns)| {
                columns.sort_by_key(|(position, _, _)| *position);
                ForeignKeyShape {
                    source_table,
                    source_columns: columns
                        .iter()
                        .map(|(_, source, _)| source.clone())
                        .collect(),
                    target_table,
                    target_columns: columns.into_iter().map(|(_, _, target)| target).collect(),
                    on_delete,
                }
            },
        )
        .collect())
}

pub(super) fn unique_constraints(
    client: &mut Client,
) -> Result<BTreeSet<UniqueConstraintShape>, MigrationError> {
    let rows = client
        .query(
            r"SELECT relation.relname AS table_name,
       con.conname AS constraint_name,
       attribute.attname AS column_name,
       key.position::bigint AS position
FROM pg_constraint con
JOIN pg_class relation ON relation.oid = con.conrelid
JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS key(number, position) ON true
JOIN pg_attribute attribute
  ON attribute.attrelid = relation.oid AND attribute.attnum = key.number
WHERE namespace.nspname = current_schema()
  AND con.contype = 'u'
  AND cardinality(con.conkey) > 1
ORDER BY relation.relname, con.conname, key.position",
            &[],
        )
        .map_err(|error| database_error("cannot inspect unique constraints", &error))?;
    let mut grouped = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        let key = (
            catalog_string(&row, "table_name")?,
            catalog_string(&row, "constraint_name")?,
        );
        let position = row
            .try_get::<_, i64>("position")
            .map_err(|error| database_error("invalid unique-constraint position", &error))?;
        grouped
            .entry(key)
            .or_default()
            .push((position, catalog_string(&row, "column_name")?));
    }
    Ok(grouped
        .into_iter()
        .map(|((table, _), mut columns)| {
            columns.sort_by_key(|(position, _)| *position);
            UniqueConstraintShape {
                table,
                columns: columns.into_iter().map(|(_, column)| column).collect(),
            }
        })
        .collect())
}

pub(super) fn indexes(client: &mut Client) -> Result<BTreeSet<IndexShape>, MigrationError> {
    let rows = client
        .query(
            r"SELECT relation.relname AS table_name,
       index_relation.relname AS index_name,
       index_data.indisunique AS is_unique,
       pg_get_expr(index_data.indpred, index_data.indrelid) AS predicate,
       attribute.attname AS column_name,
       key.position::bigint AS position
FROM pg_index index_data
JOIN pg_class relation ON relation.oid = index_data.indrelid
JOIN pg_class index_relation ON index_relation.oid = index_data.indexrelid
JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
JOIN LATERAL unnest(index_data.indkey) WITH ORDINALITY AS key(number, position) ON true
JOIN pg_attribute attribute
  ON attribute.attrelid = relation.oid AND attribute.attnum = key.number
WHERE namespace.nspname = current_schema()
  AND index_data.indisvalid
  AND key.number > 0
  AND NOT EXISTS (
    SELECT 1 FROM pg_constraint constraint_data
    WHERE constraint_data.conindid = index_data.indexrelid
  )
ORDER BY relation.relname, index_relation.relname, key.position",
            &[],
        )
        .map_err(|error| database_error("cannot inspect database indexes", &error))?;
    let mut grouped = BTreeMap::<_, Vec<_>>::new();
    for row in rows {
        let key = (
            catalog_string(&row, "table_name")?,
            catalog_string(&row, "index_name")?,
            row.try_get::<_, bool>("is_unique").map_err(|error| {
                database_error("invalid index uniqueness catalog value", &error)
            })?,
            row.try_get::<_, Option<String>>("predicate")
                .map_err(|error| database_error("invalid index predicate catalog value", &error))?,
        );
        let position = row
            .try_get::<_, i64>("position")
            .map_err(|error| database_error("invalid index position", &error))?;
        grouped
            .entry(key)
            .or_default()
            .push((position, catalog_string(&row, "column_name")?));
    }
    Ok(grouped
        .into_iter()
        .map(|((table, _, unique, predicate), mut columns)| {
            columns.sort_by_key(|(position, _)| *position);
            IndexShape {
                table,
                columns: columns.into_iter().map(|(_, column)| column).collect(),
                unique,
                predicate: predicate.map(|value| value.trim().to_owned()),
            }
        })
        .collect())
}
