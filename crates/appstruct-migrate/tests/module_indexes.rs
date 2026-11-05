use appstruct_compiler::compile_project;
use appstruct_migrate::{
    ExecutionRisk, IndexSchema, SchemaChange, SchemaRisk, connect_database, diff, extract,
    initial_migration, migration_sql,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

type ExpectedIndex<'a> = (&'a str, &'a str, &'a [&'a str], Option<&'a str>);

const JOBS_MODULE_INDEXES: &[ExpectedIndex<'_>] = &[
    (
        "appstruct::product::inbox",
        "_appstruct_product_notifications",
        &["user_id", "tenant_id", "created_at", "id"],
        None,
    ),
    (
        "appstruct::product::ledger",
        "_appstruct_product_credit_ledger",
        &["tenant_id", "expires_at"],
        None,
    ),
    (
        "appstruct::product::events",
        "_appstruct_product_events",
        &["kind", "occurred_at", "actor_id"],
        None,
    ),
    (
        "appstruct::product::retention",
        "_appstruct_product_events",
        &["actor_id", "kind", "occurred_at"],
        None,
    ),
    (
        "appstruct::product::payments",
        "_appstruct_product_payments",
        &["payment_intent"],
        None,
    ),
    (
        "appstruct::product::purchases",
        "_appstruct_product_credit_purchases",
        &["payment_intent"],
        None,
    ),
    (
        "appstruct::auth::auth_accounts_created",
        "_appstruct_auth_accounts",
        &["created_at", "user_id"],
        None,
    ),
    (
        "appstruct::auth::auth_sessions_user",
        "_appstruct_auth_sessions",
        &["user_id", "revoked_at", "expires_at"],
        None,
    ),
    (
        "appstruct::auth::auth_api_tokens_user",
        "_appstruct_auth_api_tokens",
        &["user_id", "created_at", "id"],
        None,
    ),
    (
        "appstruct::auth::auth_password_resets_user",
        "_appstruct_auth_password_resets",
        &["user_id", "used_at"],
        None,
    ),
    (
        "appstruct::auth::auth_email_verifications_user",
        "_appstruct_auth_email_verifications",
        &["user_id", "used_at"],
        None,
    ),
    (
        "appstruct::auth::saved_views_owner",
        "_appstruct_saved_views",
        &["owner_id", "scope_key", "resource", "updated_at", "id"],
        None,
    ),
    (
        "appstruct::auth::saved_views_team",
        "_appstruct_saved_views",
        &["tenant_id", "resource", "visibility", "updated_at", "id"],
        None,
    ),
    (
        "appstruct::tenant::tenant_memberships_user",
        "_appstruct_tenant_memberships",
        &["user_id", "organization_id"],
        None,
    ),
    (
        "appstruct::tenant::tenant_invitations_organization",
        "_appstruct_tenant_invitations",
        &["organization_id", "created_at", "id"],
        None,
    ),
    (
        "appstruct::jobs::jobs_queued",
        "_appstruct_jobs",
        &["run_at", "id"],
        Some("status = 'queued'"),
    ),
    (
        "appstruct::jobs::jobs_running_lease",
        "_appstruct_jobs",
        &["locked_until", "id"],
        Some("status = 'running'"),
    ),
    (
        "appstruct::jobs::jobs_admin_timeline",
        "_appstruct_jobs",
        &["status", "created_at", "id"],
        None,
    ),
    (
        "appstruct::webhooks::webhooks_pending",
        "_appstruct_webhook_deliveries",
        &["next_attempt_at", "id"],
        Some("status = 'pending'"),
    ),
    (
        "appstruct::webhooks::webhooks_running_lease",
        "_appstruct_webhook_deliveries",
        &["locked_until", "id"],
        Some("status = 'delivering'"),
    ),
    (
        "appstruct::webhooks::webhooks_admin_timeline",
        "_appstruct_webhook_deliveries",
        &["status", "created_at", "id"],
        None,
    ),
    (
        "appstruct::realtime::realtime_presence_scope",
        "_appstruct_realtime_presence",
        &[
            "tenant_id",
            "resource",
            "record_id",
            "connected_at",
            "connection_id",
        ],
        None,
    ),
    (
        "appstruct::realtime::realtime_presence_expiry",
        "_appstruct_realtime_presence",
        &["expires_at"],
        None,
    ),
    (
        "appstruct::realtime::realtime_events_expiry",
        "_appstruct_realtime_events",
        &["occurred_at"],
        None,
    ),
];

#[test]
fn jobs_fixture_emits_complete_operational_index_contract() {
    let indexes = fixture_module_indexes("m6-jobs-project");
    for &(id, table, columns, predicate) in JOBS_MODULE_INDEXES {
        let index = indexes.get(id).unwrap_or_else(|| {
            panic!("missing module index {id}");
        });
        assert_eq!(index.table, table, "index {id} table");
        assert_eq!(index.columns, columns, "index {id} columns");
        assert!(!index.unique, "index {id} uniqueness");
        assert_eq!(
            index.predicate.as_deref(),
            predicate,
            "index {id} predicate"
        );
    }
    assert_eq!(indexes.len(), JOBS_MODULE_INDEXES.len());
}

#[test]
fn saas_preset_emits_both_audit_timeline_indexes() {
    let indexes = fixture_module_indexes("m6-preset-project");
    assert!(
        indexes.contains_key("appstruct::audit::audit_events_timeline"),
        "missing audit timeline index"
    );
    assert!(
        indexes.contains_key("appstruct::audit::audit_events_tenant_timeline"),
        "missing tenant audit timeline index"
    );
}

#[test]
fn product_demo_schema_round_trips_postgres_without_index_drift() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else {
        return;
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo");
    let expected = extract(&compile_project(&project).unwrap()).unwrap();
    let schema = format!("product_index_roundtrip_{}", std::process::id());
    let mut database = connect_database(&url).unwrap();
    database
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; SET search_path TO {schema};"
        ))
        .unwrap();
    database
        .batch_execute(&initial_migration(&expected))
        .unwrap();
    let drift = appstruct_migrate::inspect_schema_structure(&url, &schema, &expected).unwrap();
    database
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .unwrap();
    assert_eq!(drift, appstruct_migrate::DriftStatus::Clean);
}

#[test]
fn tenant_sort_indexes_generate_reviewable_partial_and_full_migrations() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-tenant-project");
    let project = tempfile::tempdir().unwrap();
    fs::create_dir(project.path().join("spec")).unwrap();
    for relative in ["appstruct.yaml", "spec/identity.yaml", "spec/project.yaml"] {
        fs::copy(fixture.join(relative), project.path().join(relative)).unwrap();
    }
    let spec = project.path().join("spec/project.yaml");
    let source = fs::read_to_string(&spec).unwrap();
    fs::write(
        &spec,
        source
            .replacen(
                "    tenant: true\n",
                "    tenant: true\n    soft_delete: true\n",
                1,
            )
            .replacen(
                "    fields:\n",
                "    fields:\n      deleted_at:\n        type: datetime\n        column: removed_at\n",
                1,
            ),
    )
    .unwrap();
    let before = extract(&compile_project(project.path()).unwrap()).unwrap();
    let config = project.path().join("appstruct.yaml");
    let source = fs::read_to_string(&config).unwrap();
    fs::write(
        config,
        source.replace(
            "  tenant:\n    enabled: true\n",
            "  tenant:\n    enabled: true\n    sortable_indexes: true\n",
        ),
    )
    .unwrap();
    let after = extract(&compile_project(project.path()).unwrap()).unwrap();
    let plan = diff(&before, &after);
    assert_eq!(plan.changes.len(), 3);
    assert!(plan.changes.iter().all(|change| {
        matches!(change.change, SchemaChange::AddIndex { .. })
            && change.risk.schema == SchemaRisk::NonDestructive
            && change.risk.execution == ExecutionRisk::MayLock
    }));
    assert!(plan.is_blocked());
    assert!(migration_sql(&plan).is_err());
    let sql = initial_migration(&after);
    assert!(sql.contains("CREATE INDEX \"idx_Project_tenant_sort_name\" ON \"projects\" (\"tenant_id\", \"name\", \"id\") WHERE (\"removed_at\" IS NULL);"));
    assert!(sql.contains("CREATE INDEX \"idx_Task_tenant_sort_created_at\" ON \"tasks\" (\"tenant_id\", \"created_at\", \"id\");"));
    assert_live_tenant_indexes(&sql);
}

fn assert_live_tenant_indexes(sql: &str) {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else {
        return;
    };
    let mut database = connect_database(&url).unwrap();
    let schema = format!("tenant_sort_indexes_{}", std::process::id());
    database
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; SET search_path TO {schema};"
        ))
        .unwrap();
    database.batch_execute(sql).unwrap();
    let indexes = database
        .query(
            "SELECT indexname, indexdef FROM pg_indexes WHERE schemaname = $1",
            &[&schema],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
        .collect::<BTreeMap<_, _>>();
    database
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .unwrap();
    let projects = &indexes["idx_Project_tenant_sort_name"];
    assert!(projects.contains("(tenant_id, name, id)"));
    assert!(projects.contains("WHERE (removed_at IS NULL)"));
    let tasks = &indexes["idx_Task_tenant_sort_created_at"];
    assert!(tasks.contains("(tenant_id, created_at, id)"));
    assert!(!tasks.contains(" WHERE "));
}

fn fixture_module_indexes(name: &str) -> BTreeMap<String, IndexSchema> {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    extract(&compile_project(&project).unwrap())
        .unwrap()
        .indexes
        .into_iter()
        .filter(|index| index.id.starts_with("appstruct::"))
        .map(|index| (index.id.clone(), index))
        .collect()
}
