use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const EVENTS: &str = "_appstruct_audit_events";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![TableSchema {
        id: "appstruct::audit::events".to_owned(),
        name: EVENTS.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, true),
            column("entity", DatabaseType::Text, false, false),
            column("record_id", DatabaseType::Text, false, false),
            column(
                "operation",
                DatabaseType::Enum {
                    values: vec![
                        "create".to_owned(),
                        "update".to_owned(),
                        "delete".to_owned(),
                        "restore".to_owned(),
                    ],
                },
                false,
                false,
            ),
            column("actor_id", DatabaseType::Uuid, true, false),
            column("tenant_id", DatabaseType::Uuid, true, false),
            column("before", DatabaseType::Json, true, false),
            column("after", DatabaseType::Json, true, false),
            column("occurred_at", DatabaseType::Datetime, false, false),
        ],
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let user = ir
        .entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
        .expect("audit module requires a resolved auth user");
    let user_key = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated auth user key");
    let mut keys = vec![foreign_key(
        "actor",
        "actor_id",
        &user.table_name,
        &user_key.column_name,
    )];
    if ir.tenant.enabled {
        keys.push(foreign_key(
            "tenant",
            "tenant_id",
            "_appstruct_tenant_organizations",
            "id",
        ));
    }
    keys
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::audit::events.{name}"),
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
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::audit::{id}"),
        source_table: EVENTS.to_owned(),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete: OnDeleteIr::SetNull,
    }
}
