use super::{ColumnSchema, DatabaseType, TableSchema};
use appstruct_ir::GeneratedValueIr;

pub(super) fn table() -> TableSchema {
    TableSchema {
        id: "appstruct::imports::batches".to_owned(),
        name: "_appstruct_import_batches".to_owned(),
        columns: vec![
            column("scope", DatabaseType::Text, false, true),
            column("request_digest", DatabaseType::Text, false, false),
            column("result", DatabaseType::Json, true, false),
            ColumnSchema {
                generated: Some(GeneratedValueIr::Now),
                ..column("created_at", DatabaseType::Datetime, false, false)
            },
        ],
    }
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::imports::batches.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}
