use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const ENTRIES: &str = "_appstruct_activity_entries";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![TableSchema {
        id: "appstruct::activity::entries".to_owned(),
        name: ENTRIES.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, true),
            column("resource", DatabaseType::Text, false, false),
            column("record_id", DatabaseType::Text, false, false),
            column("tenant_id", DatabaseType::Uuid, true, false),
            column("actor_id", DatabaseType::Uuid, true, false),
            column(
                "kind",
                DatabaseType::Enum {
                    values: vec!["comment".to_owned(), "system".to_owned()],
                },
                false,
                false,
            ),
            column("body", DatabaseType::Text, true, false),
            column("event", DatabaseType::Text, true, false),
            column("payload", DatabaseType::Json, true, false),
            column("attachment_file_id", DatabaseType::Uuid, true, false),
            column("attachment_name", DatabaseType::Text, true, false),
            column("attachment_content_type", DatabaseType::Text, true, false),
            column("withdrawn_at", DatabaseType::Datetime, true, false),
            column("withdrawn_by", DatabaseType::Uuid, true, false),
            column("governance_reason", DatabaseType::Text, true, false),
            column("occurred_at", DatabaseType::Datetime, false, false),
        ],
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let user = ir
        .entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
        .expect("activity requires a resolved auth user");
    let user_key = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated auth user key");
    let mut keys = vec![
        foreign_key(
            "actor",
            "actor_id",
            &user.table_name,
            &user_key.column_name,
            OnDeleteIr::SetNull,
        ),
        foreign_key(
            "withdrawn_by",
            "withdrawn_by",
            &user.table_name,
            &user_key.column_name,
            OnDeleteIr::SetNull,
        ),
    ];
    if ir.tenant.enabled {
        keys.push(foreign_key(
            "tenant",
            "tenant_id",
            "_appstruct_tenant_organizations",
            "id",
            OnDeleteIr::Restrict,
        ));
    }
    if ir.file.enabled {
        keys.push(foreign_key(
            "attachment_file",
            "attachment_file_id",
            "_appstruct_files",
            "id",
            OnDeleteIr::SetNull,
        ));
    }
    keys
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::activity::entries.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}

fn foreign_key(
    id: &str,
    source_column: &str,
    target_table: &str,
    target_column: &str,
    on_delete: OnDeleteIr,
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::activity::{id}"),
        source_table: ENTRIES.to_owned(),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete,
    }
}
