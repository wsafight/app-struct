use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const ORGANIZATIONS: &str = "_appstruct_tenant_organizations";
const MEMBERSHIPS: &str = "_appstruct_tenant_memberships";
const INVITATIONS: &str = "_appstruct_tenant_invitations";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![organizations(), memberships(), invitations()]
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
        foreign_key(
            "invitation_organization",
            INVITATIONS,
            "organization_id",
            ORGANIZATIONS,
            "id",
            OnDeleteIr::Cascade,
        ),
        foreign_key(
            "invitation_inviter",
            INVITATIONS,
            "invited_by",
            user_table,
            user_key,
            OnDeleteIr::Restrict,
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

fn invitations() -> TableSchema {
    TableSchema {
        id: "appstruct::tenant::invitations".to_owned(),
        name: INVITATIONS.to_owned(),
        columns: vec![
            column("invitations.id", "id", DatabaseType::Uuid, true),
            column(
                "invitations.organization_id",
                "organization_id",
                DatabaseType::Uuid,
                false,
            ),
            column("invitations.email", "email", DatabaseType::Text, false),
            ColumnSchema {
                id: "appstruct::tenant::invitations.role".to_owned(),
                name: "role".to_owned(),
                data_type: DatabaseType::Text,
                nullable: false,
                primary_key: false,
                unique: false,
                default: None,
                generated: None,
            },
            ColumnSchema {
                id: "appstruct::tenant::invitations.token_hash".to_owned(),
                name: "token_hash".to_owned(),
                data_type: DatabaseType::Text,
                nullable: false,
                primary_key: false,
                unique: true,
                default: None,
                generated: None,
            },
            column(
                "invitations.expires_at",
                "expires_at",
                DatabaseType::Datetime,
                false,
            ),
            ColumnSchema {
                id: "appstruct::tenant::invitations.accepted_at".to_owned(),
                name: "accepted_at".to_owned(),
                data_type: DatabaseType::Datetime,
                nullable: true,
                primary_key: false,
                unique: false,
                default: None,
                generated: None,
            },
            column(
                "invitations.invited_by",
                "invited_by",
                DatabaseType::Uuid,
                false,
            ),
            column(
                "invitations.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
            ),
        ],
    }
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
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete,
    }
}
