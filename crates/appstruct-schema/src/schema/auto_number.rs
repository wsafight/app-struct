use super::{ColumnSchema, DatabaseType, TableSchema, UniqueConstraintSchema};
use appstruct_ir::{AppIr, GeneratedValueIr};

pub(super) fn enabled(ir: &AppIr) -> bool {
    ir.entities.iter().any(|entity| {
        entity
            .fields
            .iter()
            .any(|field| matches!(field.generated, Some(GeneratedValueIr::AutoNumber)))
    })
}

pub(super) fn table() -> TableSchema {
    TableSchema {
        id: "appstruct::auto_number::sequences".to_owned(),
        name: "_appstruct_number_sequences".to_owned(),
        columns: vec![
            column("scope", DatabaseType::Text, false, true),
            column("next_value", DatabaseType::Bigint, false, false),
            ColumnSchema {
                generated: Some(GeneratedValueIr::Now),
                ..column("updated_at", DatabaseType::Datetime, false, false)
            },
        ],
    }
}

pub(super) fn unique_constraints(ir: &AppIr) -> Vec<UniqueConstraintSchema> {
    ir.entities
        .iter()
        .flat_map(|entity| {
            entity.fields.iter().filter_map(|field| {
                if !matches!(field.generated, Some(GeneratedValueIr::AutoNumber)) {
                    return None;
                }
                if entity.tenant_scoped {
                    Some(UniqueConstraintSchema {
                        id: format!("{}.{}.auto_number", entity.id, field.column_name),
                        table: entity.table_name.clone(),
                        columns: vec!["tenant_id".to_owned(), field.column_name.clone()],
                    })
                } else if !field.unique {
                    Some(UniqueConstraintSchema {
                        id: format!("{}.{}.auto_number", entity.id, field.column_name),
                        table: entity.table_name.clone(),
                        columns: vec![field.column_name.clone()],
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::auto_number::sequences.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}
