use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, GeneratedValueIr, OnDeleteIr};

const PRESENCE: &str = "_appstruct_realtime_presence";
const EVENTS: &str = "_appstruct_realtime_events";
const LOCKS: &str = "_appstruct_realtime_locks";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![presence_table(), events_table(), locks_table()]
}

fn presence_table() -> TableSchema {
    TableSchema {
        id: "appstruct::realtime::presence".to_owned(),
        name: PRESENCE.to_owned(),
        columns: vec![
            column(
                PRESENCE,
                "connection_id",
                DatabaseType::Uuid,
                false,
                true,
                false,
            ),
            column(
                PRESENCE,
                "actor_id",
                DatabaseType::Uuid,
                false,
                false,
                false,
            ),
            column(
                PRESENCE,
                "tenant_id",
                DatabaseType::Uuid,
                true,
                false,
                false,
            ),
            column(PRESENCE, "resource", DatabaseType::Text, true, false, false),
            column(
                PRESENCE,
                "record_id",
                DatabaseType::Text,
                true,
                false,
                false,
            ),
            column(
                PRESENCE,
                "connected_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
            column(
                PRESENCE,
                "last_seen_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
            column(
                PRESENCE,
                "expires_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
        ],
    }
}

fn events_table() -> TableSchema {
    TableSchema {
        id: "appstruct::realtime::events".to_owned(),
        name: EVENTS.to_owned(),
        columns: vec![
            generated_sequence(),
            column(EVENTS, "id", DatabaseType::Uuid, false, false, true),
            column(EVENTS, "source_id", DatabaseType::Uuid, false, false, false),
            column(EVENTS, "event", DatabaseType::Text, false, false, false),
            column(EVENTS, "data", DatabaseType::Json, false, false, false),
            column(EVENTS, "resource", DatabaseType::Text, true, false, false),
            column(EVENTS, "record_id", DatabaseType::Text, true, false, false),
            column(EVENTS, "actor_id", DatabaseType::Uuid, true, false, false),
            column(EVENTS, "tenant_id", DatabaseType::Uuid, true, false, false),
            column(
                EVENTS,
                "occurred_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
            column(
                EVENTS,
                "resource_model",
                DatabaseType::Boolean,
                false,
                false,
                false,
            ),
        ],
    }
}

fn locks_table() -> TableSchema {
    TableSchema {
        id: "appstruct::realtime::locks".to_owned(),
        name: LOCKS.to_owned(),
        columns: vec![
            column(LOCKS, "lock_key", DatabaseType::Text, false, true, false),
            column(LOCKS, "lease_token", DatabaseType::Uuid, false, false, true),
            column(LOCKS, "actor_id", DatabaseType::Uuid, false, false, false),
            column(LOCKS, "tenant_id", DatabaseType::Uuid, true, false, false),
            column(LOCKS, "resource", DatabaseType::Text, false, false, false),
            column(LOCKS, "record_id", DatabaseType::Text, false, false, false),
            column(
                LOCKS,
                "acquired_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
            column(
                LOCKS,
                "expires_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
        ],
    }
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
    let mut keys = vec![
        foreign_key(
            PRESENCE,
            "actor",
            "actor_id",
            &user.table_name,
            &user_key.column_name,
            OnDeleteIr::Cascade,
        ),
        foreign_key(
            LOCKS,
            "lock_actor",
            "actor_id",
            &user.table_name,
            &user_key.column_name,
            OnDeleteIr::Cascade,
        ),
    ];
    if ir.tenant.enabled {
        keys.push(foreign_key(
            PRESENCE,
            "tenant",
            "tenant_id",
            "_appstruct_tenant_organizations",
            "id",
            OnDeleteIr::Cascade,
        ));
        keys.push(foreign_key(
            LOCKS,
            "lock_tenant",
            "tenant_id",
            "_appstruct_tenant_organizations",
            "id",
            OnDeleteIr::Cascade,
        ));
    }
    keys
}

fn column(
    table: &str,
    name: &str,
    data_type: DatabaseType,
    nullable: bool,
    primary_key: bool,
    unique: bool,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::realtime::{table}.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique,
        default: None,
        generated: None,
    }
}

fn generated_sequence() -> ColumnSchema {
    let mut sequence = column(EVENTS, "sequence", DatabaseType::Bigint, false, true, false);
    sequence.generated = Some(GeneratedValueIr::AutoIncrement);
    sequence
}

fn foreign_key(
    source_table: &str,
    id: &str,
    source_column: &str,
    target_table: &str,
    target_column: &str,
    on_delete: OnDeleteIr,
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::realtime::{id}"),
        source_table: source_table.to_owned(),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete,
    }
}
