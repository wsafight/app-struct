use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const FILES: &str = "_appstruct_files";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![TableSchema {
        id: "appstruct::file::files".to_owned(),
        name: FILES.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, true, false),
            column("object_key", DatabaseType::Text, false, false, true),
            column("original_name", DatabaseType::Text, false, false, false),
            column("content_type", DatabaseType::Text, false, false, false),
            column("size", DatabaseType::Bigint, false, false, false),
            column("checksum", DatabaseType::Text, false, false, false),
            column("tenant_id", DatabaseType::Uuid, true, false, false),
            column("created_at", DatabaseType::Datetime, false, false, false),
        ],
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    if !ir.tenant.enabled {
        return Vec::new();
    }
    vec![ForeignKeySchema {
        id: "appstruct::file::file_tenant".to_owned(),
        source_table: FILES.to_owned(),
        source_column: "tenant_id".to_owned(),
        target_table: "_appstruct_tenant_organizations".to_owned(),
        target_column: "id".to_owned(),
        unique: false,
        on_delete: OnDeleteIr::SetNull,
    }]
}

fn column(
    name: &str,
    data_type: DatabaseType,
    nullable: bool,
    primary_key: bool,
    unique: bool,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::file::files.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique,
        default: None,
        generated: None,
    }
}
