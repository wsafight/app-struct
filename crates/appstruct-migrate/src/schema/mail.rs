use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const DELIVERIES: &str = "_appstruct_mail_deliveries";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![TableSchema {
        id: "appstruct::mail::deliveries".to_owned(),
        name: DELIVERIES.to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, true),
            column("provider", DatabaseType::Text, false, false),
            column("template", DatabaseType::Text, false, false),
            column("sender", DatabaseType::Text, false, false),
            column("recipient", DatabaseType::Text, false, false),
            column("subject", DatabaseType::Text, false, false),
            column("text_body", DatabaseType::Text, false, false),
            column("html_body", DatabaseType::Text, true, false),
            column("tenant_id", DatabaseType::Uuid, true, false),
            column("created_at", DatabaseType::Datetime, false, false),
        ],
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    if !ir.tenant.enabled {
        return Vec::new();
    }
    vec![ForeignKeySchema {
        id: "appstruct::mail::delivery_tenant".to_owned(),
        source_table: DELIVERIES.to_owned(),
        source_column: "tenant_id".to_owned(),
        target_table: "_appstruct_tenant_organizations".to_owned(),
        target_column: "id".to_owned(),
        unique: false,
        on_delete: OnDeleteIr::SetNull,
    }]
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::mail::deliveries.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}
