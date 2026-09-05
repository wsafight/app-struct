mod support;

use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::{fs, path::Path};
use support::{assert_rustfmt, cargo_check};

#[test]
fn jobs_contract_generates_a_compilable_backend() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-jobs-project");
    let artifacts = plan(&compile_project(&fixture).unwrap()).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    write_artifacts(temporary.path(), &artifacts);

    assert_database_contracts(&artifacts);
    assert_backend_contracts(&artifacts);
    assert_interface_contracts(&artifacts);

    let manifest = temporary.path().join("generated/backend/Cargo.toml");
    assert_rustfmt(&manifest);
    let checked = cargo_check(&manifest, false);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

fn assert_database_contracts(artifacts: &[Artifact]) {
    let sql = artifact_text(artifacts, "database/0001_initial.sql");
    assert!(sql.contains("_appstruct_jobs"));
    assert!(sql.contains("_appstruct_job_schedules"));
    assert!(sql.contains("\"paused\" BOOLEAN NOT NULL DEFAULT false"));
    assert!(sql.contains("\"interval_seconds\" BIGINT"));
    assert!(sql.contains("_appstruct_webhook_deliveries"));
    assert!(sql.contains("_appstruct_realtime_presence"));
    assert!(sql.contains("_appstruct_realtime_events"));
    assert!(sql.contains("_appstruct_realtime_locks"));
    assert!(sql.contains("\"idempotency_key\" TEXT UNIQUE"));
    assert!(sql.contains("FOREIGN KEY (\"tenant_id\")"));
}

fn assert_backend_contracts(artifacts: &[Artifact]) {
    let jobs = artifact_text(artifacts, "backend/src/jobs.rs");
    assert!(jobs.contains("FOR UPDATE SKIP LOCKED"));
    assert!(jobs.contains("schedule:{}:{}"));
    assert!(jobs.contains("maintenance.cleanup"));
    assert!(jobs.contains("schedule_due"));
    assert!(jobs.contains("SET enabled = FALSE WHERE enabled"));
    assert!(jobs.contains("SET enabled = TRUE WHERE name = $1"));
    assert!(jobs.contains("cron::Schedule"));
    assert!(jobs.contains("next_schedule_run"));
    assert!(jobs.contains("CURRENT_TIMESTAMP AS scheduler_now"));
    assert!(jobs.contains("AND NOT paused"));
    assert!(jobs.contains("IS NOT DISTINCT FROM EXCLUDED.interval_seconds"));
    assert!(jobs.contains("enqueue(\n            &transaction"));
    assert!(artifact_text(artifacts, "backend/Cargo.toml").contains("cron = \"=0.15.0\""));
    let webhooks = artifact_text(artifacts, "backend/src/webhooks.rs");
    assert!(webhooks.contains("x-appstruct-signature"));
    assert!(webhooks.contains("Hmac::<Sha256>"));
    assert!(webhooks.contains("FOR UPDATE SKIP LOCKED"));
    assert!(webhooks.contains("project.created"));
    assert!(webhooks.contains("connect_timeout(Duration::from_millis(200"));
    assert!(webhooks.contains("read_timeout(Duration::from_millis(300"));
    assert!(webhooks.contains("timeout(Duration::from_millis(500"));
    assert!(webhooks.contains("request timed out"));
    let realtime = artifact_text(artifacts, "backend/src/realtime.rs");
    assert!(realtime.contains("/api/realtime/events"));
    assert!(realtime.contains("presence.online"));
    assert!(realtime.contains("authorize_resource_scope"));
    assert!(realtime.contains("authorize_resource_event"));
    assert!(realtime.contains("event.resource.as_deref() != Some(resource)"));
    assert!(realtime.contains("realtime resource is required"));
    assert!(realtime.contains("if event.resource_model"));
    assert!(realtime.contains("realtime fan-out poll failed"));
    assert!(realtime.contains("WHERE sequence > $1 ORDER BY sequence LIMIT 256"));
    let project_api = artifact_text(artifacts, "backend/src/api/project.rs");
    assert!(project_api.contains("&state.realtime"));
    assert!(project_api.contains("RequestContext::connection_with_services"));
    assert!(project_api.contains("\"project.created\""));
    assert!(project_api.contains("publish_realtime_event(&state, &context"));
    assert!(project_api.contains("publish_resource_model"));
    assert!(project_api.contains("authorize_realtime_scope"));
    assert!(project_api.contains("authorize_realtime_event"));
    assert!(jobs.contains("status = 'running' AND locked_until <= CURRENT_TIMESTAMP"));
    assert!(jobs.contains("pub struct JobWorkerHandle"));
    assert!(jobs.contains("SupervisedTaskHandle::spawn"));
    assert!(jobs.contains("APPSTRUCT_JOB_CONCURRENCY"));
    assert!(jobs.contains("tokio::task::JoinSet"));
    assert!(!jobs.contains("WorkerExitGuard"));
    assert!(jobs.contains("pub async fn shutdown"));
    assert!(jobs.contains("pub fn for_kind"));
    assert!(jobs.contains("pub struct MailJobPayload"));
    assert!(jobs.contains("impl JobHandler for MailJobHandler"));
    assert!(webhooks.contains("APPSTRUCT_WEBHOOK_CONCURRENCY"));
    assert!(webhooks.contains("tokio::task::JoinSet"));
    assert!(webhooks.contains("client: self.client.clone()"));
    let session = artifact_text(artifacts, "backend/src/auth/session.rs");
    assert!(session.contains("value.starts_with(\"Bearer \")"));
}

fn assert_interface_contracts(artifacts: &[Artifact]) {
    let admin = artifact_text(artifacts, "backend/src/auth/admin.rs");
    assert!(admin.contains("/api/admin/jobs/{id}/retry"));
    assert!(admin.contains("Only dead jobs can be retried"));
    assert!(admin.contains("Only succeeded or dead jobs can be replayed"));
    let schedules_admin = artifact_text(artifacts, "backend/src/auth/admin_schedules.rs");
    assert!(schedules_admin.contains("/api/admin/schedules/{id}/pause"));
    assert!(schedules_admin.contains("trigger_schedule"));
    let client = artifact_text(artifacts, "web/src/generated/client.ts");
    assert!(client.contains("listJobs"));
    assert!(client.contains("retryJob"));
    assert!(client.contains("replayJob"));
    assert!(client.contains("listSchedules"));
    assert!(client.contains("pauseSchedule"));
    assert!(client.contains("triggerSchedule"));
    assert!(
        artifact_text(artifacts, "web/src/auth/AdminSchedulesPage.tsx")
            .contains("AdminSchedulesPage")
    );
    assert!(client.contains("subscribeRealtime"));
    let realtime_hook = artifact_text(artifacts, "web/src/realtime/useRealtimeResource.ts");
    assert!(realtime_hook.contains("import { subscribeRealtime }"));
    assert!(!realtime_hook.contains("import(\"../generated/client\")"));
    assert!(client.contains("listPresence"));
    assert!(client.contains("acquireRealtimeLock"));
    assert!(client.contains("renewRealtimeLock"));
    assert!(client.contains("releaseRealtimeLock"));
    assert!(client.contains("listWebhooks"));
    assert!(client.contains("retryWebhook"));
    assert!(client.contains("replayWebhook"));
    let openapi: serde_json::Value =
        serde_json::from_str(artifact_text(artifacts, "openapi/openapi.json")).unwrap();
    assert!(openapi["paths"]["/api/admin/jobs"]["get"].is_object());
    assert!(openapi["paths"]["/api/admin/jobs/{id}/retry"]["post"].is_object());
    assert!(openapi["paths"]["/api/admin/jobs/{id}/replay"]["post"].is_object());
    assert!(openapi["paths"]["/api/admin/schedules"]["get"].is_object());
    assert!(openapi["paths"]["/api/admin/schedules/{id}/pause"]["post"].is_object());
    assert!(openapi["paths"]["/api/admin/schedules/{id}/trigger"]["post"].is_object());
    assert!(openapi["paths"]["/api/realtime/events"]["get"].is_object());
    assert!(openapi["paths"]["/api/realtime/presence"]["get"].is_object());
    assert!(openapi["paths"]["/api/realtime/locks"]["post"].is_object());
    assert!(openapi["paths"]["/api/realtime/locks/{token}"]["patch"].is_object());
    assert!(openapi["paths"]["/api/admin/webhooks"]["get"].is_object());
    assert!(openapi["paths"]["/api/admin/webhooks/{id}/retry"]["post"].is_object());
    let extensions = artifact_text(artifacts, "backend/src/extensions.rs");
    assert!(extensions.contains("pub async fn enqueue_job"));
    assert!(extensions.contains("pub async fn publish_webhook"));
    assert!(extensions.contains("pub fn publish_realtime"));
    assert!(extensions.contains("pub fn job_handler"));
    let library = artifact_text(artifacts, "backend/src/lib.rs");
    assert!(library.contains("pub struct Application"));
    assert!(library.contains("shutdown signal received"));
    assert!(!library.contains("expect(\"invalid AppStruct"));
    let main = artifact_text(artifacts, "backend/src/main.rs");
    assert!(main.contains("Application::from_env"));
    assert!(main.contains("application.serve(listener)"));
}

fn artifact_text<'artifacts>(artifacts: &'artifacts [Artifact], path: &str) -> &'artifacts str {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .unwrap();
    std::str::from_utf8(&artifact.content).unwrap()
}

fn write_artifacts(root: &Path, artifacts: &[Artifact]) {
    for artifact in artifacts {
        let destination = root.join("generated").join(&artifact.relative_path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &artifact.content).unwrap();
    }
}
