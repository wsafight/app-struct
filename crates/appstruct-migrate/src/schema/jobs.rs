use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const JOBS: &str = "_appstruct_jobs";
const SCHEDULES: &str = "_appstruct_job_schedules";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![
        TableSchema {
            id: "appstruct::jobs::jobs".to_owned(),
            name: JOBS.to_owned(),
            columns: vec![
                column("id", DatabaseType::Uuid, false, true, false),
                column("queue", DatabaseType::Text, false, false, false),
                column("kind", DatabaseType::Text, false, false, false),
                column("payload", DatabaseType::Json, false, false, false),
                column("idempotency_key", DatabaseType::Text, true, false, true),
                column("tenant_id", DatabaseType::Uuid, true, false, false),
                column(
                    "status",
                    DatabaseType::Enum {
                        values: ["queued", "running", "succeeded", "dead"]
                            .into_iter()
                            .map(str::to_owned)
                            .collect(),
                    },
                    false,
                    false,
                    false,
                ),
                column("attempts", DatabaseType::Integer, false, false, false),
                column("max_attempts", DatabaseType::Integer, false, false, false),
                column("backoff_seconds", DatabaseType::Bigint, false, false, false),
                column("run_at", DatabaseType::Datetime, false, false, false),
                column("locked_by", DatabaseType::Text, true, false, false),
                column("locked_until", DatabaseType::Datetime, true, false, false),
                column("last_error", DatabaseType::Text, true, false, false),
                column("created_at", DatabaseType::Datetime, false, false, false),
                column("completed_at", DatabaseType::Datetime, true, false, false),
            ],
        },
        TableSchema {
            id: "appstruct::jobs::schedules".to_owned(),
            name: SCHEDULES.to_owned(),
            columns: vec![
                column("id", DatabaseType::Uuid, false, true, false),
                column("name", DatabaseType::Text, false, false, true),
                column("cron", DatabaseType::Text, false, false, false),
                column(
                    "interval_seconds",
                    DatabaseType::Bigint,
                    false,
                    false,
                    false,
                ),
                column("queue", DatabaseType::Text, false, false, false),
                column("kind", DatabaseType::Text, false, false, false),
                column("payload", DatabaseType::Json, false, false, false),
                column("enabled", DatabaseType::Boolean, false, false, false),
                column("next_run_at", DatabaseType::Datetime, false, false, false),
                column("last_run_at", DatabaseType::Datetime, true, false, false),
                column("created_at", DatabaseType::Datetime, false, false, false),
            ],
        },
    ]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    if !ir.tenant.enabled {
        return Vec::new();
    }
    vec![ForeignKeySchema {
        id: "appstruct::jobs::job_tenant".to_owned(),
        source_table: JOBS.to_owned(),
        source_columns: vec!["tenant_id".to_owned()],
        target_table: "_appstruct_tenant_organizations".to_owned(),
        target_columns: vec!["id".to_owned()],
        unique: false,
        on_delete: OnDeleteIr::SetNull,
    }]
}

fn column(
    name: &str,
    data_type: DatabaseType,
    nullable: bool,
    primary_key: bool,
    unique: bool,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::jobs::jobs.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique,
        default: None,
        generated: None,
    }
}
