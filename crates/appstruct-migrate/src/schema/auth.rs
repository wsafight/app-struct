use appstruct_ir::{AppIr, OnDeleteIr};

use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};

pub(super) fn tables() -> Vec<TableSchema> {
    vec![
        accounts(),
        sessions(),
        password_resets(),
        email_verifications(),
        oauth_accounts(),
        mail_capture(),
    ]
}

fn accounts() -> TableSchema {
    table(
        "accounts",
        vec![
            column(
                "accounts.user_id",
                "user_id",
                DatabaseType::Uuid,
                false,
                true,
            ),
            column(
                "accounts.password_hash",
                "password_hash",
                DatabaseType::Text,
                false,
                false,
            ),
            column("accounts.roles", "roles", DatabaseType::Json, false, false),
            ColumnSchema {
                id: "appstruct::auth::accounts.email_verified_at".to_owned(),
                name: "email_verified_at".to_owned(),
                data_type: DatabaseType::Datetime,
                nullable: true,
                primary_key: false,
                unique: false,
                default: None,
                generated: None,
            },
            column(
                "accounts.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
        ],
    )
}

fn sessions() -> TableSchema {
    table(
        "sessions",
        vec![
            column(
                "sessions.token_hash",
                "token_hash",
                DatabaseType::Text,
                false,
                true,
            ),
            column(
                "sessions.user_id",
                "user_id",
                DatabaseType::Uuid,
                false,
                false,
            ),
            column(
                "sessions.csrf_hash",
                "csrf_hash",
                DatabaseType::Text,
                false,
                false,
            ),
            column(
                "sessions.expires_at",
                "expires_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
            column(
                "sessions.revoked_at",
                "revoked_at",
                DatabaseType::Datetime,
                true,
                false,
            ),
            column(
                "sessions.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
        ],
    )
}

fn password_resets() -> TableSchema {
    table(
        "password_resets",
        vec![
            column(
                "password_resets.token_hash",
                "token_hash",
                DatabaseType::Text,
                false,
                true,
            ),
            column(
                "password_resets.user_id",
                "user_id",
                DatabaseType::Uuid,
                false,
                false,
            ),
            column(
                "password_resets.expires_at",
                "expires_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
            column(
                "password_resets.used_at",
                "used_at",
                DatabaseType::Datetime,
                true,
                false,
            ),
            column(
                "password_resets.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
        ],
    )
}

fn mail_capture() -> TableSchema {
    table(
        "mail_capture",
        vec![
            column("mail_capture.id", "id", DatabaseType::Uuid, false, true),
            column(
                "mail_capture.recipient",
                "recipient",
                DatabaseType::Text,
                false,
                false,
            ),
            column(
                "mail_capture.subject",
                "subject",
                DatabaseType::Text,
                false,
                false,
            ),
            column(
                "mail_capture.body",
                "body",
                DatabaseType::Text,
                false,
                false,
            ),
            column(
                "mail_capture.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
        ],
    )
}

fn email_verifications() -> TableSchema {
    table(
        "email_verifications",
        vec![
            column(
                "email_verifications.token_hash",
                "token_hash",
                DatabaseType::Text,
                false,
                true,
            ),
            column(
                "email_verifications.user_id",
                "user_id",
                DatabaseType::Uuid,
                false,
                false,
            ),
            column(
                "email_verifications.expires_at",
                "expires_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
            ColumnSchema {
                id: "appstruct::auth::email_verifications.used_at".to_owned(),
                name: "used_at".to_owned(),
                data_type: DatabaseType::Datetime,
                nullable: true,
                primary_key: false,
                unique: false,
                default: None,
                generated: None,
            },
            column(
                "email_verifications.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
        ],
    )
}

fn oauth_accounts() -> TableSchema {
    table(
        "oauth_accounts",
        vec![
            column(
                "oauth_accounts.provider",
                "provider",
                DatabaseType::Text,
                false,
                false,
            ),
            column(
                "oauth_accounts.subject",
                "subject",
                DatabaseType::Text,
                false,
                true,
            ),
            column(
                "oauth_accounts.user_id",
                "user_id",
                DatabaseType::Uuid,
                false,
                false,
            ),
            column(
                "oauth_accounts.created_at",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
            ),
        ],
    )
}

fn table(name: &str, columns: Vec<ColumnSchema>) -> TableSchema {
    TableSchema {
        id: format!("appstruct::auth::{name}"),
        name: format!("_appstruct_auth_{name}"),
        columns,
    }
}

fn column(
    id: &str,
    name: &str,
    data_type: DatabaseType,
    nullable: bool,
    primary_key: bool,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::auth::{id}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique: false,
        default: None,
        generated: None,
    }
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let user = ir
        .entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
        .expect("compiler resolved auth user entity");
    let user_key = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated auth user key");

    vec![
        foreign_key(
            "account_user",
            "accounts",
            "user_id",
            &user.table_name,
            &user_key.column_name,
        ),
        foreign_key(
            "session_user",
            "sessions",
            "user_id",
            "_appstruct_auth_accounts",
            "user_id",
        ),
        foreign_key(
            "reset_user",
            "password_resets",
            "user_id",
            "_appstruct_auth_accounts",
            "user_id",
        ),
        foreign_key(
            "email_verification_user",
            "email_verifications",
            "user_id",
            "_appstruct_auth_accounts",
            "user_id",
        ),
        foreign_key(
            "oauth_account_user",
            "oauth_accounts",
            "user_id",
            "_appstruct_auth_accounts",
            "user_id",
        ),
    ]
}

fn foreign_key(
    id: &str,
    source: &str,
    source_column: &str,
    target_table: &str,
    target_column: &str,
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::auth::{id}"),
        source_table: format!("_appstruct_auth_{source}"),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete: OnDeleteIr::Cascade,
    }
}
