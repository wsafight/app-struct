use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const PRESENCE: &str = "_appstruct_realtime_presence";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![TableSchema {
        id: "appstruct::realtime::presence".to_owned(),
        name: PRESENCE.to_owned(),
        columns: vec![
            column("connection_id", DatabaseType::Uuid, false, true),
            column("actor_id", DatabaseType::Uuid, false, false),
            column("tenant_id", DatabaseType::Uuid, true, false),
            column("resource", DatabaseType::Text, true, false),
            column("record_id", DatabaseType::Text, true, false),
            column("connected_at", DatabaseType::Datetime, false, false),
            column("last_seen_at", DatabaseType::Datetime, false, false),
            column("expires_at", DatabaseType::Datetime, false, false),
        ],
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let user = ir
        .entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
        .expect("realtime requires a resolved auth user");
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
        OnDeleteIr::Cascade,
    )];
    if ir.tenant.enabled {
        keys.push(foreign_key(
            "tenant",
            "tenant_id",
            "_appstruct_tenant_organizations",
            "id",
            OnDeleteIr::Cascade,
        ));
    }
    keys
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::realtime::presence.{name}"),
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
        id: format!("appstruct::realtime::{id}"),
        source_table: PRESENCE.to_owned(),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete,
    }
}
