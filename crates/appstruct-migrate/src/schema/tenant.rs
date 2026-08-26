use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const ORGANIZATIONS: &str = "_appstruct_tenant_organizations";
const MEMBERSHIPS: &str = "_appstruct_tenant_memberships";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![organizations(), memberships()]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let (user_table, user_key) = auth_user(ir);
    let mut keys = vec![
        foreign_key(
            "organization_creator",
            ORGANIZATIONS,
            "created_by",
            user_table,
            user_key,
            OnDeleteIr::Restrict,
        ),
        foreign_key(
            "membership_organization",
            MEMBERSHIPS,
            "organization_id",
            ORGANIZATIONS,
            "id",
            OnDeleteIr::Cascade,
        ),
        foreign_key(
            "membership_user",
            MEMBERSHIPS,
            "user_id",
            user_table,
            user_key,
            OnDeleteIr::Cascade,
        ),
    ];
    keys.extend(
        ir.entities
            .iter()
            .filter(|entity| entity.tenant_scoped)
            .map(|entity| {
                foreign_key(
                    &format!("{}_tenant", entity.table_name),
                    &entity.table_name,
                    "tenant_id",
                    ORGANIZATIONS,
                    "id",
                    OnDeleteIr::Cascade,
                )
            }),
    );
    keys
}

fn organizations() -> TableSchema {
    table(
        "organizations",
        ORGANIZATIONS,
        vec![
            column("organizations.id", "id", DatabaseType::Uuid, true),
            column("organizations.name", "name", DatabaseType::Text, false),
            column(
                "organizations.created_by",
                "created_by",
                DatabaseType::Uuid,
                false,
            ),
            column(
                "organizations.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
            ),
        ],
    )
}

fn memberships() -> TableSchema {
    table(
        "memberships",
        MEMBERSHIPS,
        vec![
            column(
                "memberships.organization_id",
                "organization_id",
                DatabaseType::Uuid,
                true,
            ),
            column("memberships.user_id", "user_id", DatabaseType::Uuid, true),
            column("memberships.role", "role", DatabaseType::Text, false),
            column(
                "memberships.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
            ),
        ],
    )
}

fn table(id: &str, name: &str, columns: Vec<ColumnSchema>) -> TableSchema {
    TableSchema {
        id: format!("appstruct::tenant::{id}"),
        name: name.to_owned(),
        columns,
    }
}

fn column(id: &str, name: &str, data_type: DatabaseType, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::tenant::{id}"),
        name: name.to_owned(),
        data_type,
        nullable: false,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}

fn auth_user(ir: &AppIr) -> (&str, &str) {
    let user = ir
        .entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
        .expect("tenant module requires a resolved auth user");
    let key = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated auth user key");
    (&user.table_name, &key.column_name)
}

fn foreign_key(
    id: &str,
    source_table: &str,
    source_column: &str,
    target_table: &str,
    target_column: &str,
    on_delete: OnDeleteIr,
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::tenant::{id}"),
        source_table: source_table.to_owned(),
        source_column: source_column.to_owned(),
        target_table: target_table.to_owned(),
        target_column: target_column.to_owned(),
        unique: false,
        on_delete,
    }
}
