use appstruct_ir::{AppIr, DatabaseProvider, FieldTypeIr, GeneratedValueIr, OnDeleteIr};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub schema_version: u32,
    pub provider: DatabaseProvider,
    pub tables: Vec<TableSchema>,
    pub foreign_keys: Vec<ForeignKeySchema>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
    pub id: String,
    pub name: String,
    pub columns: Vec<ColumnSchema>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub id: String,
    pub name: String,
    pub data_type: DatabaseType,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub default: Option<String>,
    pub generated: Option<GeneratedValueIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatabaseType {
    Uuid,
    Text,
    Integer,
    Bigint,
    Decimal,
    Boolean,
    Date,
    Datetime,
    Json,
    Enum { values: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeySchema {
    pub id: String,
    pub source_table: String,
    pub source_column: String,
    pub target_table: String,
    pub target_column: String,
    pub unique: bool,
    pub on_delete: OnDeleteIr,
}

#[must_use]
pub fn extract(ir: &AppIr) -> DatabaseSchema {
    let tables = ir
        .entities
        .iter()
        .map(|entity| TableSchema {
            id: entity.id.0.clone(),
            name: entity.table_name.clone(),
            columns: entity
                .fields
                .iter()
                .map(|field| ColumnSchema {
                    id: field.id.0.clone(),
                    name: field.column_name.clone(),
                    data_type: database_type(ir, &field.ty),
                    nullable: field.nullable,
                    primary_key: field.primary_key,
                    unique: field.unique,
                    default: field.default.clone(),
                    generated: field.generated,
                })
                .collect(),
        })
        .collect();
    let foreign_keys = ir
        .relations
        .iter()
        .map(|relation| foreign_key(ir, relation))
        .collect();
    DatabaseSchema {
        schema_version: 1,
        provider: ir.database.provider,
        tables,
        foreign_keys,
    }
}

fn database_type(ir: &AppIr, field_type: &FieldTypeIr) -> DatabaseType {
    match field_type {
        FieldTypeIr::Uuid => DatabaseType::Uuid,
        FieldTypeIr::String | FieldTypeIr::Text => DatabaseType::Text,
        FieldTypeIr::Integer => DatabaseType::Integer,
        FieldTypeIr::Bigint => DatabaseType::Bigint,
        FieldTypeIr::Decimal => DatabaseType::Decimal,
        FieldTypeIr::Boolean => DatabaseType::Boolean,
        FieldTypeIr::Date => DatabaseType::Date,
        FieldTypeIr::Datetime => DatabaseType::Datetime,
        FieldTypeIr::Json => DatabaseType::Json,
        FieldTypeIr::Enum { values } => DatabaseType::Enum {
            values: values.clone(),
        },
        FieldTypeIr::Relation { target } => {
            let target = ir
                .entities
                .iter()
                .find(|entity| entity.id == *target)
                .expect("compiler resolved relation target");
            let key = target
                .fields
                .iter()
                .find(|field| field.primary_key)
                .expect("compiler validated primary key");
            database_type(ir, &key.ty)
        }
    }
}

fn foreign_key(ir: &AppIr, relation: &appstruct_ir::RelationIr) -> ForeignKeySchema {
    let source = ir
        .entities
        .iter()
        .find(|entity| entity.id == relation.source)
        .expect("compiler resolved relation source");
    let target = ir
        .entities
        .iter()
        .find(|entity| entity.id == relation.target)
        .expect("compiler resolved relation target");
    let source_field = source
        .fields
        .iter()
        .find(|field| relation.foreign_key_fields.contains(&field.id))
        .expect("compiler resolved relation field");
    let target_field = target
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated target key");
    ForeignKeySchema {
        id: relation.id.0.clone(),
        source_table: source.table_name.clone(),
        source_column: source_field.column_name.clone(),
        target_table: target.table_name.clone(),
        target_column: target_field.column_name.clone(),
        unique: relation.unique,
        on_delete: relation.on_delete,
    }
}

/// Serialize a canonical schema snapshot.
///
/// # Errors
///
/// Returns an error if JSON serialization unexpectedly fails.
pub fn to_json(schema: &DatabaseSchema) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_string_pretty(schema)?;
    value.push('\n');
    Ok(value)
}

/// Parse a schema snapshot.
///
/// # Errors
///
/// Returns an error if the snapshot is invalid or incompatible JSON.
pub fn from_json(source: &str) -> Result<DatabaseSchema, serde_json::Error> {
    serde_json::from_str(source)
}
