use appstruct_compiler::compile_project;
use appstruct_migrate::extract;
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn saas_modules_emit_operational_indexes() {
    let preset = fixture_index_ids("m6-preset-project");
    let jobs = fixture_index_ids("m6-jobs-project");

    for expected in [
        "appstruct::auth::auth_sessions_user",
        "appstruct::tenant::tenant_memberships_user",
        "appstruct::jobs::jobs_queued",
        "appstruct::realtime::realtime_presence_scope",
    ] {
        assert!(jobs.contains(expected), "missing module index {expected}");
    }
    assert!(
        preset.contains("appstruct::audit::audit_events_tenant_timeline"),
        "missing audit timeline index"
    );
    assert!(
        jobs.contains("appstruct::webhooks::webhooks_pending"),
        "missing webhook delivery index"
    );
}

fn fixture_index_ids(name: &str) -> BTreeSet<String> {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    extract(&compile_project(&project).unwrap())
        .unwrap()
        .indexes
        .iter()
        .map(|index| index.id.clone())
        .collect()
}
