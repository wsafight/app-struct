use super::{ColumnSchema, DatabaseType, ForeignKeySchema, TableSchema, UniqueConstraintSchema};
use appstruct_ir::{AppIr, OnDeleteIr};

const TEMPLATES: &str = "_appstruct_report_templates";
const RUNS: &str = "_appstruct_report_runs";

pub(super) fn tables() -> Vec<TableSchema> {
    vec![template_table(), run_table()]
}

fn template_table() -> TableSchema {
    TableSchema {
        id: "appstruct::report::templates".to_owned(),
        name: TEMPLATES.to_owned(),
        columns: vec![
            column("templates", "id", DatabaseType::Uuid, false, true, false),
            column("templates", "name", DatabaseType::Text, false, false, false),
            column(
                "templates",
                "version",
                DatabaseType::Integer,
                false,
                false,
                false,
            ),
            column(
                "templates",
                "document_type",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column("templates", "body", DatabaseType::Text, false, false, false),
            column(
                "templates",
                "artifact_digest",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column(
                "templates",
                "input_schema",
                DatabaseType::Json,
                false,
                false,
                false,
            ),
            column(
                "templates",
                "data_schema_version",
                DatabaseType::Integer,
                false,
                false,
                false,
            ),
            column(
                "templates",
                "renderer_version",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column(
                "templates",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
        ],
    }
}

#[allow(clippy::too_many_lines)]
fn run_table() -> TableSchema {
    TableSchema {
        id: "appstruct::report::runs".to_owned(),
        name: RUNS.to_owned(),
        columns: vec![
            column("runs", "id", DatabaseType::Uuid, false, true, false),
            column(
                "runs",
                "execution_job_id",
                DatabaseType::Uuid,
                true,
                false,
                true,
            ),
            column(
                "runs",
                "template_id",
                DatabaseType::Uuid,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "template_version",
                DatabaseType::Integer,
                false,
                false,
                false,
            ),
            column("runs", "tenant_id", DatabaseType::Uuid, true, false, false),
            column("runs", "actor_id", DatabaseType::Uuid, true, false, false),
            column(
                "runs",
                "idempotency_scope",
                DatabaseType::Text,
                false,
                false,
                true,
            ),
            column(
                "runs",
                "idempotency_key",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "request_digest",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "snapshot_ciphertext",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "snapshot_digest",
                DatabaseType::Text,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "snapshot_size",
                DatabaseType::Bigint,
                false,
                false,
                false,
            ),
            column("runs", "locale", DatabaseType::Text, false, false, false),
            column("runs", "timezone", DatabaseType::Text, false, false, false),
            column(
                "runs",
                "paper",
                enum_type(&["a4", "letter"]),
                false,
                false,
                false,
            ),
            column(
                "runs",
                "orientation",
                enum_type(&["portrait", "landscape"]),
                false,
                false,
                false,
            ),
            column(
                "runs",
                "stage",
                enum_type(&[
                    "queued",
                    "rendering",
                    "publishing",
                    "succeeded",
                    "failed",
                    "cancelled",
                ]),
                false,
                false,
                false,
            ),
            column(
                "runs",
                "progress",
                DatabaseType::Integer,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "result_file_id",
                DatabaseType::Uuid,
                true,
                false,
                true,
            ),
            column(
                "runs",
                "result_object_key",
                DatabaseType::Text,
                true,
                false,
                true,
            ),
            column("runs", "error_code", DatabaseType::Text, true, false, false),
            column(
                "runs",
                "created_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
            column(
                "runs",
                "completed_at",
                DatabaseType::Datetime,
                true,
                false,
                false,
            ),
            column(
                "runs",
                "expires_at",
                DatabaseType::Datetime,
                false,
                false,
                false,
            ),
        ],
    }
}

pub(super) fn unique_constraints() -> Vec<UniqueConstraintSchema> {
    vec![UniqueConstraintSchema {
        id: "appstruct::report::template_identity".to_owned(),
        table: TEMPLATES.to_owned(),
        columns: vec!["name".to_owned(), "version".to_owned()],
    }]
}

pub(super) fn foreign_keys(ir: &AppIr) -> Vec<ForeignKeySchema> {
    let user = ir
        .entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
        .expect("report requires a resolved auth user");
    let user_key = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .expect("compiler validated auth user key");
    let mut keys = vec![
        foreign_key(
            "run_job",
            "execution_job_id",
            "_appstruct_jobs",
            "id",
            OnDeleteIr::Restrict,
        ),
        foreign_key(
            "run_template",
            "template_id",
            TEMPLATES,
            "id",
            OnDeleteIr::Restrict,
        ),
        foreign_key(
            "run_actor",
            "actor_id",
            &user.table_name,
            &user_key.column_name,
            OnDeleteIr::SetNull,
        ),
        foreign_key(
            "run_file",
            "result_file_id",
            "_appstruct_files",
            "id",
            OnDeleteIr::SetNull,
        ),
    ];
    if ir.tenant.enabled {
        keys.push(foreign_key(
            "run_tenant",
            "tenant_id",
            "_appstruct_tenant_organizations",
            "id",
            OnDeleteIr::Restrict,
        ));
    }
    keys
}

fn foreign_key(
    id: &str,
    source_column: &str,
    target_table: &str,
    target_column: &str,
    on_delete: OnDeleteIr,
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("appstruct::report::{id}"),
        source_table: RUNS.to_owned(),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete,
    }
}

fn enum_type(values: &[&str]) -> DatabaseType {
    DatabaseType::Enum {
        values: values.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn column(
    owner: &str,
    name: &str,
    data_type: DatabaseType,
    nullable: bool,
    primary_key: bool,
    unique: bool,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("appstruct::report::{owner}.{name}"),
        name: name.to_owned(),
        data_type,
        nullable,
        primary_key,
        unique,
        default: None,
        generated: None,
    }
}
