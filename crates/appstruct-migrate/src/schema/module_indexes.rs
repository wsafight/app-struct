use super::IndexSchema;
use appstruct_ir::AppIr;

pub(super) fn indexes(ir: &AppIr) -> Vec<IndexSchema> {
    let mut indexes = Vec::new();
    if ir.auth.enabled {
        indexes.extend(auth());
    }
    if ir.tenant.enabled {
        indexes.extend(tenant());
    }
    if ir.audit.enabled {
        indexes.extend(audit());
    }
    if ir.jobs.enabled {
        indexes.extend(jobs());
    }
    if ir.webhooks.enabled {
        indexes.extend(webhooks());
    }
    if ir.realtime.enabled {
        indexes.extend(realtime());
    }
    if ir.report.enabled {
        indexes.extend(report());
    }
    if ir.activity.enabled {
        indexes.extend(activity());
    }
    indexes
}

fn auth() -> Vec<IndexSchema> {
    vec![
        index(
            "appstruct::auth::auth_accounts_created",
            "_appstruct_auth_accounts",
            &["created_at", "user_id"],
            None,
        ),
        index(
            "appstruct::auth::auth_sessions_user",
            "_appstruct_auth_sessions",
            &["user_id", "revoked_at", "expires_at"],
            None,
        ),
        index(
            "appstruct::auth::auth_api_tokens_user",
            "_appstruct_auth_api_tokens",
            &["user_id", "created_at", "id"],
            None,
        ),
        index(
            "appstruct::auth::auth_password_resets_user",
            "_appstruct_auth_password_resets",
            &["user_id", "used_at"],
            None,
        ),
        index(
            "appstruct::auth::auth_email_verifications_user",
            "_appstruct_auth_email_verifications",
            &["user_id", "used_at"],
            None,
        ),
        index(
            "appstruct::auth::saved_views_owner",
            "_appstruct_saved_views",
            &["owner_id", "scope_key", "resource", "updated_at", "id"],
            None,
        ),
        index(
            "appstruct::auth::saved_views_team",
            "_appstruct_saved_views",
            &["tenant_id", "resource", "visibility", "updated_at", "id"],
            None,
        ),
    ]
}

fn tenant() -> [IndexSchema; 2] {
    [
        index(
            "appstruct::tenant::tenant_memberships_user",
            "_appstruct_tenant_memberships",
            &["user_id", "organization_id"],
            None,
        ),
        index(
            "appstruct::tenant::tenant_invitations_organization",
            "_appstruct_tenant_invitations",
            &["organization_id", "created_at", "id"],
            None,
        ),
    ]
}

fn audit() -> [IndexSchema; 2] {
    [
        index(
            "appstruct::audit::audit_events_timeline",
            "_appstruct_audit_events",
            &["occurred_at", "id"],
            None,
        ),
        index(
            "appstruct::audit::audit_events_tenant_timeline",
            "_appstruct_audit_events",
            &["tenant_id", "occurred_at", "id"],
            None,
        ),
    ]
}

fn jobs() -> [IndexSchema; 3] {
    [
        index(
            "appstruct::jobs::jobs_queued",
            "_appstruct_jobs",
            &["run_at", "id"],
            Some("status = 'queued'"),
        ),
        index(
            "appstruct::jobs::jobs_running_lease",
            "_appstruct_jobs",
            &["locked_until", "id"],
            Some("status = 'running'"),
        ),
        index(
            "appstruct::jobs::jobs_admin_timeline",
            "_appstruct_jobs",
            &["status", "created_at", "id"],
            None,
        ),
    ]
}

fn webhooks() -> [IndexSchema; 3] {
    [
        index(
            "appstruct::webhooks::webhooks_pending",
            "_appstruct_webhook_deliveries",
            &["next_attempt_at", "id"],
            Some("status = 'pending'"),
        ),
        index(
            "appstruct::webhooks::webhooks_running_lease",
            "_appstruct_webhook_deliveries",
            &["locked_until", "id"],
            Some("status = 'delivering'"),
        ),
        index(
            "appstruct::webhooks::webhooks_admin_timeline",
            "_appstruct_webhook_deliveries",
            &["status", "created_at", "id"],
            None,
        ),
    ]
}

fn realtime() -> [IndexSchema; 3] {
    [
        index(
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
        index(
            "appstruct::realtime::realtime_presence_expiry",
            "_appstruct_realtime_presence",
            &["expires_at"],
            None,
        ),
        index(
            "appstruct::realtime::realtime_events_expiry",
            "_appstruct_realtime_events",
            &["occurred_at"],
            None,
        ),
    ]
}

fn report() -> [IndexSchema; 3] {
    [
        index(
            "appstruct::report::runs_actor_timeline",
            "_appstruct_report_runs",
            &["tenant_id", "actor_id", "created_at", "id"],
            None,
        ),
        index(
            "appstruct::report::runs_stage_timeline",
            "_appstruct_report_runs",
            &["tenant_id", "stage", "created_at", "id"],
            None,
        ),
        index(
            "appstruct::report::runs_expiry",
            "_appstruct_report_runs",
            &["expires_at", "id"],
            None,
        ),
    ]
}

fn activity() -> [IndexSchema; 3] {
    [
        index(
            "appstruct::activity::record_timeline",
            "_appstruct_activity_entries",
            &["tenant_id", "resource", "record_id", "occurred_at", "id"],
            None,
        ),
        index(
            "appstruct::activity::actor_timeline",
            "_appstruct_activity_entries",
            &["tenant_id", "actor_id", "occurred_at", "id"],
            None,
        ),
        index(
            "appstruct::activity::governance_timeline",
            "_appstruct_activity_entries",
            &["tenant_id", "withdrawn_at", "id"],
            Some("withdrawn_at IS NOT NULL"),
        ),
    ]
}

fn index(id: &str, table: &str, columns: &[&str], predicate: Option<&str>) -> IndexSchema {
    IndexSchema {
        id: id.to_owned(),
        table: table.to_owned(),
        columns: columns.iter().map(|column| (*column).to_owned()).collect(),
        unique: false,
        predicate: predicate.map(str::to_owned),
    }
}
