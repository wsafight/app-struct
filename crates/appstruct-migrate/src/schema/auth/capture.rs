use super::{column, table};
use crate::schema::{DatabaseType, TableSchema};

pub(super) fn table_schema() -> TableSchema {
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
