use super::super::{
    ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema, UniqueConstraintSchema,
};
use appstruct_ir::{AppIr, OnDeleteIr};

pub(super) fn table_schema() -> TableSchema {
    TableSchema {
        id: "appstruct::auth::saved_views".to_owned(),
        name: "_appstruct_saved_views".to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, false, true),
            column("owner_id", DatabaseType::Uuid, false, false),
            column("scope_key", DatabaseType::Text, false, false),
            column("tenant_id", DatabaseType::Uuid, true, false),
            column("resource", DatabaseType::Text, false, false),
            column("name", DatabaseType::Text, false, false),
            column("query", DatabaseType::Text, false, false),
            column("visibility", DatabaseType::Text, false, false),
            default_column("revision", DatabaseType::Bigint, "1"),
            column("created_at", DatabaseType::Datetime, false, false),
            column("updated_at", DatabaseType::Datetime, false, false),
        ],
    }
}

pub(super) fn unique_constraints() -> Vec<UniqueConstraintSchema> {
    vec![UniqueConstraintSchema {
        id: "appstruct::auth::saved_views_owner_scope_name".to_owned(),
        table: "_appstruct_saved_views".to_owned(),
        columns: ["owner_id", "scope_key", "resource", "name"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let mut foreign_keys = vec![ForeignKeySchema {
        id: "appstruct::auth::saved_view_owner".to_owned(),
        source_table: "_appstruct_saved_views".to_owned(),
        source_columns: vec!["owner_id".to_owned()],
        target_table: "_appstruct_auth_accounts".to_owned(),
        target_columns: vec!["user_id".to_owned()],
        unique: false,
        on_delete: OnDeleteIr::Cascade,
    }];
    if ir.tenant.enabled {
        foreign_keys.push(ForeignKeySchema {
            id: "appstruct::auth::saved_view_tenant".to_owned(),
            source_table: "_appstruct_saved_views".to_owned(),
            source_columns: vec!["tenant_id".to_owned()],
            target_table: "_appstruct_tenant_organizations".to_owned(),
            target_columns: vec!["id".to_owned()],
            unique: false,
            on_delete: OnDeleteIr::Cascade,
        });
    }
    foreign_keys
}

fn column(name: &str, data_type: DatabaseType, nullable: bool, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::auth::saved_views.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}

fn default_column(name: &str, data_type: DatabaseType, value: &str) -> ColumnSchema {
    let mut column = column(name, data_type, false, false);
    column.default = Some(value.to_owned());
    column
}
