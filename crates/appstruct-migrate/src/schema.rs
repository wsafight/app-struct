use appstruct_ir::{
    AppIr, DatabaseProvider, FieldTypeIr, GeneratedValueIr, IrValidationErrors, OnDeleteIr,
    validate_app_ir,
};
use serde::{Deserialize, Serialize};

mod audit;
mod auth;
mod file;
mod jobs;
mod mail;
mod module_indexes;
mod realtime;
mod tenant;
mod webhooks;

pub const SCHEMA_VERSION: u32 = appstruct_contracts::DATABASE_SCHEMA.current;
pub const MIN_COMPATIBLE_SCHEMA_VERSION: u32 = appstruct_contracts::DATABASE_SCHEMA.minimum;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub schema_version: u32,
    pub provider: DatabaseProvider,
    pub tables: Vec<TableSchema>,
    #[serde(default)]
    pub unique_constraints: Vec<UniqueConstraintSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<IndexSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seeds: Vec<SeedSchema>,
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
    #[serde(alias = "source_column", deserialize_with = "one_or_many")]
    pub source_columns: Vec<String>,
    pub target_table: String,
    #[serde(alias = "target_column", deserialize_with = "one_or_many")]
    pub target_columns: Vec<String>,
    pub unique: bool,
    pub on_delete: OnDeleteIr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueConstraintSchema {
    pub id: String,
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSchema {
    pub id: String,
    pub table: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedSchema {
    pub id: String,
    pub table: String,
    pub values: Vec<SeedValueSchema>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedValueSchema {
    pub column: String,
    pub value: String,
    pub data_type: DatabaseType,
}

/// Extract a database schema from semantically valid IR.
///
/// # Errors
///
/// Returns all IR invariant violations before schema traversal begins.
pub fn extract(ir: &AppIr) -> Result<DatabaseSchema, IrValidationErrors> {
    validate_app_ir(ir)?;
    let mut tables = ir
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
        .collect::<Vec<_>>();
    let unique_constraints = ir
        .entities
        .iter()
        .filter(|entity| entity.tenant_scoped)
        .map(tenant_unique_constraint)
        .collect::<Vec<_>>();
    let mut indexes = entity_indexes(ir);
    indexes.extend(module_indexes::indexes(ir));
    let seeds = entity_seeds(ir);
    let mut foreign_keys = ir
        .relations
        .iter()
        .map(|relation| foreign_key(ir, relation))
        .collect::<Vec<_>>();
    if ir.auth.enabled {
        tables.extend(auth::tables());
        foreign_keys.extend(auth::foreign_keys(ir));
    }
    if ir.tenant.enabled {
        tables.extend(tenant::tables());
        foreign_keys.extend(tenant::foreign_keys(ir));
    }
    if ir.audit.enabled {
        tables.extend(audit::tables());
        foreign_keys.extend(audit::foreign_keys(ir));
    }
    if ir.mail.enabled {
        tables.extend(mail::tables());
        foreign_keys.extend(mail::foreign_keys(ir));
    }
    if ir.jobs.enabled {
        tables.extend(jobs::tables());
        foreign_keys.extend(jobs::foreign_keys(ir));
    }
    if ir.webhooks.enabled {
        tables.extend(webhooks::tables());
        foreign_keys.extend(webhooks::foreign_keys(ir));
    }
    if ir.realtime.enabled {
        tables.extend(realtime::tables());
        foreign_keys.extend(realtime::foreign_keys(ir));
    }
    if ir.file.enabled {
        tables.extend(file::tables());
        foreign_keys.extend(file::foreign_keys(ir));
    }
    Ok(DatabaseSchema {
        schema_version: SCHEMA_VERSION,
        provider: ir.database.provider,
        tables,
        unique_constraints,
        indexes,
        seeds,
        foreign_keys,
    })
}

fn entity_indexes(ir: &AppIr) -> Vec<IndexSchema> {
    ir.entities
        .iter()
        .flat_map(|entity| {
            entity.indexes.iter().map(|index| IndexSchema {
                id: index.id.clone(),
                table: entity.table_name.clone(),
                columns: index
                    .fields
                    .iter()
                    .filter_map(|field_id| {
                        entity
                            .fields
                            .iter()
                            .find(|field| field.id == *field_id)
                            .map(|field| field.column_name.clone())
                    })
                    .collect(),
                unique: index.unique,
                predicate: index.predicate.clone(),
            })
        })
        .collect()
}

fn entity_seeds(ir: &AppIr) -> Vec<SeedSchema> {
    ir.seeds
        .iter()
        .filter_map(|seed| {
            let entity = ir.entities.iter().find(|entity| entity.id == seed.entity)?;
            let values = seed
                .values
                .iter()
                .filter_map(|(field_name, value)| {
                    let field = entity.fields.iter().find(|field| {
                        field.api_name == *field_name || field.rust_name == *field_name
                    })?;
                    Some(SeedValueSchema {
                        column: field.column_name.clone(),
                        value: value.clone(),
                        data_type: database_type(ir, &field.ty),
                    })
                })
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(SeedSchema {
                id: seed.id.clone(),
                table: entity.table_name.clone(),
                values,
            })
        })
        .collect()
}

fn tenant_unique_constraint(entity: &appstruct_ir::EntityIr) -> UniqueConstraintSchema {
    let key = entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated primary key");
    UniqueConstraintSchema {
        id: format!("{}.tenant_key", entity.id),
        table: entity.table_name.clone(),
        columns: vec!["tenant_id".to_owned(), key.column_name.clone()],
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
    let tenant_relation = source.tenant_scoped && target.tenant_scoped;
    ForeignKeySchema {
        id: relation.id.0.clone(),
        source_table: source.table_name.clone(),
        source_columns: if tenant_relation {
            vec!["tenant_id".to_owned(), source_field.column_name.clone()]
        } else {
            vec![source_field.column_name.clone()]
        },
        target_table: target.table_name.clone(),
        target_columns: if tenant_relation {
            vec!["tenant_id".to_owned(), target_field.column_name.clone()]
        } else {
            vec![target_field.column_name.clone()]
        },
        unique: relation.unique,
        on_delete: relation.on_delete,
    }
}

fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Columns {
        One(String),
        Many(Vec<String>),
    }

    Ok(match Columns::deserialize(deserializer)? {
        Columns::One(column) => vec![column],
        Columns::Many(columns) => columns,
    })
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
    let mut schema: DatabaseSchema = serde_json::from_str(source)?;
    match schema.schema_version {
        SCHEMA_VERSION => Ok(schema),
        MIN_COMPATIBLE_SCHEMA_VERSION => {
            schema.schema_version = SCHEMA_VERSION;
            Ok(schema)
        }
        found => Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "unsupported schema snapshot version {found}; supported versions are {MIN_COMPATIBLE_SCHEMA_VERSION} through {SCHEMA_VERSION}"
            ),
        ))),
    }
}
