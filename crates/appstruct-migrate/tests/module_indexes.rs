use appstruct_compiler::compile_project;
use appstruct_migrate::{IndexSchema, extract};
use std::collections::BTreeMap;
use std::path::Path;

type ExpectedIndex<'a> = (&'a str, &'a str, &'a [&'a str], Option<&'a str>);

const JOBS_MODULE_INDEXES: &[ExpectedIndex<'_>] = &[
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
