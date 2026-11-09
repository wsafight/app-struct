use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::OnDeleteIr;

const CUSTOMERS: &str = "_appstruct_billing_customers";
const SUBSCRIPTIONS: &str = "_appstruct_billing_subscriptions";
const EVENTS: &str = "_appstruct_billing_events";
const USAGE: &str = "_appstruct_billing_usage";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![customers(), subscriptions(), events(), usage()]
}

pub(super) fn foreign_keys() -> Vec<ForeignKeySchema> {
    vec![
        foreign_key("customer_organization", CUSTOMERS),
        foreign_key("subscription_organization", SUBSCRIPTIONS),
        foreign_key("usage_organization", USAGE),
    ]
}

fn customers() -> TableSchema {
    TableSchema {
        id: "appstruct::billing::customers".to_owned(),
        name: CUSTOMERS.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, false, None),
            column("organization_id", DatabaseType::Uuid, false, true, None),
            column(
                "provider",
                DatabaseType::Text,
                false,
                false,
                Some("'stripe'"),
            ),
            column(
                "provider_customer_id",
                DatabaseType::Text,
                false,
                true,
                None,
            ),
            column("created_at", DatabaseType::Datetime, false, false, None),
        ],
    }
}

fn subscriptions() -> TableSchema {
    TableSchema {
        id: "appstruct::billing::subscriptions".to_owned(),
        name: SUBSCRIPTIONS.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, false, None),
            column("organization_id", DatabaseType::Uuid, false, false, None),
            column(
                "provider",
                DatabaseType::Text,
                false,
                false,
                Some("'stripe'"),
            ),
            column(
                "provider_subscription_id",
                DatabaseType::Text,
                false,
                true,
                None,
            ),
            column("plan_id", DatabaseType::Text, false, false, None),
            column("status", DatabaseType::Text, false, false, None),
            column(
                "current_period_end",
                DatabaseType::Datetime,
                true,
                false,
                None,
            ),
            column(
                "cancel_at_period_end",
                DatabaseType::Boolean,
                false,
                false,
                Some("false"),
            ),
            column(
                "provider_event_created_at",
                DatabaseType::Bigint,
                false,
                false,
                Some("0"),
            ),
            column("created_at", DatabaseType::Datetime, false, false, None),
            column("updated_at", DatabaseType::Datetime, false, false, None),
        ],
    }
}

fn events() -> TableSchema {
    TableSchema {
        id: "appstruct::billing::events".to_owned(),
        name: EVENTS.to_owned(),
        columns: vec![
            column("event_id", DatabaseType::Text, false, false, None),
            column("event_type", DatabaseType::Text, false, false, None),
            column("payload", DatabaseType::Json, false, false, None),
            column("created_at", DatabaseType::Datetime, false, false, None),
        ],
    }
}

fn usage() -> TableSchema {
    TableSchema {
        id: "appstruct::billing::usage".to_owned(),
        name: USAGE.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, false, None),
            column("organization_id", DatabaseType::Uuid, false, false, None),
            column("entitlement", DatabaseType::Text, false, false, None),
            column("free", DatabaseType::Boolean, false, false, Some("false")),
            column(
                "refunded",
                DatabaseType::Boolean,
                false,
                false,
                Some("false"),
            ),
            column("created_at", DatabaseType::Datetime, false, false, None),
        ],
    }
}

fn column(
    name: &str,
    data_type: DatabaseType,
    nullable: bool,
    unique: bool,
    default: Option<&str>,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::billing::{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key: name == "id" || name == "event_id",
        unique,
        default: default.map(str::to_owned),
        generated: None,
    }
}

fn foreign_key(id: &str, source_table: &str) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::billing::{id}"),
        source_table: source_table.to_owned(),
        source_columns: vec!["organization_id".to_owned()],
        target_table: "_appstruct_tenant_organizations".to_owned(),
        target_columns: vec!["id".to_owned()],
        unique: false,
        on_delete: OnDeleteIr::Cascade,
    }
}
