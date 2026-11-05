use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, GeneratedValueIr, OnDeleteIr};

const FILES: &str = "_appstruct_files";

pub(super) fn tables() -> Vec<TableSchema> {
    let mut tables = vec![TableSchema {
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
            ColumnSchema {
                default: Some("[]".to_owned()),
                ..column("reference_owners", DatabaseType::Json, false, false, false)
            },
            ColumnSchema {
                generated: Some(GeneratedValueIr::Now),
                ..column(
                    "unreferenced_at",
                    DatabaseType::Datetime,
                    true,
                    false,
                    false,
                )
            },
            ColumnSchema {
                default: Some("false".to_owned()),
                ..column("deleting", DatabaseType::Boolean, false, false, false)
            },
        ],
    }];
    tables.push(TableSchema {
        id: "appstruct::file::uploads".to_owned(),
        name: "_appstruct_file_uploads".to_owned(),
        columns: [
            ("id", DatabaseType::Uuid, false),
            ("tenant_id", DatabaseType::Uuid, true),
            ("actor_id", DatabaseType::Uuid, false),
            ("object_key", DatabaseType::Text, false),
            ("original_name", DatabaseType::Text, false),
            ("content_type", DatabaseType::Text, false),
            ("size", DatabaseType::Bigint, false),
            ("expires_at", DatabaseType::Datetime, false),
            ("file_id", DatabaseType::Uuid, true),
            ("cancelled", DatabaseType::Boolean, false),
        ]
        .into_iter()
        .map(|(name, data_type, nullable)| ColumnSchema {
            id: format!("appstruct::file::uploads.{name}"),
            name: name.to_owned(),
            data_type,
            nullable,
            primary_key: name == "id",
            unique: false,
            default: (name == "cancelled").then(|| "false".to_owned()),
            generated: None,
        })
        .collect(),
    });
    tables
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    if !ir.tenant.enabled {
        return Vec::new();
    }
    vec![ForeignKeySchema {
        id: "appstruct::file::file_tenant".to_owned(),
        source_table: FILES.to_owned(),
        source_columns: vec!["tenant_id".to_owned()],
        target_table: "_appstruct_tenant_organizations".to_owned(),
        target_columns: vec!["id".to_owned()],
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
