use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::OnDeleteIr;

pub(super) fn tables() -> Vec<TableSchema> {
    use DatabaseType::{Bigint, Boolean, Datetime, Json, Text, Uuid};
    vec![
        table(
            "profiles",
            &[
                ("user_id", Uuid, false),
                ("display_name", Text, false),
                ("locale", Text, false),
                ("timezone", Text, false),
                ("notifications", Boolean, false),
                ("mfa_secret", Text, true),
                ("mfa_pending", Text, true),
                ("mfa_pending_expires", Datetime, true),
                ("mfa_recovery_hashes", Json, false),
                ("mfa_counter", Bigint, false),
                ("email_pending", Text, true),
                ("email_token_hash", Text, true),
                ("email_expires_at", Datetime, true),
            ],
        ),
        table(
            "notifications",
            &[
                ("id", Uuid, false),
                ("user_id", Uuid, false),
                ("tenant_id", Uuid, true),
                ("event_key", Text, false),
                ("title", Text, false),
                ("body", Text, false),
                ("target", Text, false),
                ("read_at", Datetime, true),
                ("created_at", Datetime, false),
            ],
        ),
        table(
            "credit_ledger",
            &[
                ("id", Uuid, false),
                ("tenant_id", Uuid, false),
                ("actor_id", Uuid, true),
                ("operation_key", Text, false),
                ("amount", Bigint, false),
                ("reason", Text, false),
                ("expires_at", Datetime, true),
                ("created_at", Datetime, false),
            ],
        ),
        table(
            "credit_reservations",
            &[
                ("id", Uuid, false),
                ("tenant_id", Uuid, false),
                ("actor_id", Uuid, false),
                ("cost", Bigint, false),
                ("monthly", Bigint, false),
                ("paid", Bigint, false),
                ("expires_at", Datetime, false),
                ("refunded", Boolean, false),
                ("created_at", Datetime, false),
            ],
        ),
        table(
            "credit_purchases",
            &[
                ("session_id", Text, false),
                ("tenant_id", Uuid, false),
                ("credits", Bigint, false),
                ("fulfilled", Boolean, false),
                ("payment_intent", Text, true),
                ("refunded_credits", Bigint, false),
                ("created_at", Datetime, false),
            ],
        ),
        table(
            "payments",
            &[
                ("charge_id", Text, false),
                ("tenant_id", Uuid, true),
                ("payment_intent", Text, true),
                ("currency", Text, false),
                ("captured", Bigint, false),
                ("refunded", Bigint, false),
                ("paid_at", Datetime, false),
                ("updated_at", Datetime, false),
            ],
        ),
        table(
            "events",
            &[
                ("id", Uuid, false),
                ("actor_id", Uuid, true),
                ("visitor_id", Uuid, true),
                ("tenant_id", Uuid, true),
                ("event_key", Text, false),
                ("kind", Text, false),
                ("payload", Json, false),
                ("occurred_at", Datetime, false),
            ],
        ),
    ]
}

pub(super) fn foreign_keys() -> Vec<ForeignKeySchema> {
    ["profiles", "notifications"]
        .into_iter()
        .map(|name| ForeignKeySchema {
            id: format!("appstruct::product::{name}.user"),
            source_table: format!("_appstruct_product_{name}"),
            source_columns: vec!["user_id".to_owned()],
            target_table: "_appstruct_auth_accounts".to_owned(),
            target_columns: vec!["user_id".to_owned()],
            unique: false,
            on_delete: OnDeleteIr::Cascade,
        })
        .collect()
}

fn table(name: &str, fields: &[(&str, DatabaseType, bool)]) -> TableSchema {
    TableSchema {
        id: format!("appstruct::product::{name}"),
        name: format!("_appstruct_product_{name}"),
        columns: fields
            .iter()
            .enumerate()
            .map(|(index, (field, data_type, nullable))| ColumnSchema {
                id: format!("appstruct::product::{name}.{field}"),
                name: (*field).to_owned(),
                data_type: data_type.clone(),
                nullable: *nullable,
                primary_key: index == 0,
                unique: *field == "operation_key" || *field == "event_key",
                default: None,
                generated: None,
            })
            .collect(),
    }
}
