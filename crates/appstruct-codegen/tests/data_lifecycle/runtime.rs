use appstruct_generated_backend::{audit, FileError, FileProvider, FileState, MailState, RealtimeState, ReconcileMode, ReconcileOptions, RequestContext, StoredObject, database::{Database, DatabaseExecutor, Sql}};
use futures_util::{StreamExt as _, stream::BoxStream};
use sqlx::Row as _;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

#[derive(Default)]
struct Storage {
    data: Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
    modified: Mutex<std::collections::BTreeMap<String, chrono::DateTime<chrono::Utc>>>,
    fail_delete: AtomicBool,
    pause_delete: AtomicBool,
    delete_entered: tokio::sync::Notify,
    delete_resume: tokio::sync::Notify,
}
impl Storage {
    /// Write an object straight into storage with no metadata row, as a crashed upload would.
    fn plant(&self, key: &str, age_hours: i64) {
        self.data.lock().unwrap().insert(key.into(), b"x".to_vec());
        self.age(key, age_hours);
    }
    fn age(&self, key: &str, age_hours: i64) {
        self.modified.lock().unwrap().insert(key.into(), chrono::Utc::now() - chrono::Duration::hours(age_hours));
    }
}

/// A provider that implements only the required methods, so `list` keeps its default.
struct NoList;
#[async_trait::async_trait]
impl FileProvider for NoList {
    fn name(&self) -> &'static str { "no-list" }
    async fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), FileError> { Ok(()) }
    async fn get(&self, _key: &str) -> Result<Vec<u8>, FileError> { Ok(Vec::new()) }
    async fn delete(&self, _key: &str) -> Result<(), FileError> { Ok(()) }
}

#[async_trait::async_trait]
impl FileProvider for Storage {
    fn name(&self) -> &'static str { "test" }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), FileError> {
        self.data.lock().unwrap().insert(key.into(), bytes.into());
        self.modified.lock().unwrap().insert(key.into(), chrono::Utc::now());
        Ok(())
    }
    fn list(&self) -> BoxStream<'static, Result<StoredObject, FileError>> {
        let modified = self.modified.lock().unwrap().clone();
        let items: Vec<_> = self.data.lock().unwrap().iter().map(|(key, bytes)| Ok(StoredObject {
            key: key.clone(),
            size: bytes.len() as u64,
            last_modified: modified.get(key).copied().unwrap_or_else(chrono::Utc::now),
        })).collect();
        futures_util::stream::iter(items).boxed()
    }
    async fn get(&self, key: &str) -> Result<Vec<u8>, FileError> {
        self.data.lock().unwrap().get(key).cloned().ok_or_else(|| FileError::Storage("missing".into()))
    }
    async fn delete(&self, key: &str) -> Result<(), FileError> {
        if self.fail_delete.load(Ordering::SeqCst) { return Err(FileError::Storage("injected failure".into())); }
        if self.pause_delete.swap(false, Ordering::SeqCst) {
            self.delete_entered.notify_one();
            self.delete_resume.notified().await;
        }
        self.data.lock().unwrap().remove(key);
        self.modified.lock().unwrap().remove(key);
        Ok(())
    }
}

async fn database() -> Option<(Database, Database, String)> {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else { return None; };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("lifecycle_{}", uuid::Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let database = Database::connect(format!("{url}{separator}options=-csearch_path%3D{schema}")).await.unwrap();
    database.execute_unprepared(include_str!("../../database/0001_initial.sql")).await.unwrap();
    Some((root, database, schema))
}

#[tokio::test]
async fn unconfigured_retention_keeps_events_and_runs_configured_case() {
    let Some((root, db, schema)) = database().await else { return; };
    db.execute_unprepared("INSERT INTO _appstruct_audit_events (id,entity,record_id,operation,occurred_at) VALUES (gen_random_uuid(),'app::Project','1','create',CURRENT_TIMESTAMP - INTERVAL '90 days')").await.unwrap();
    assert_eq!(audit::prune_expired(&db, 100).await.unwrap(), 0);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "configured_lifecycle", "--nocapture"])
        .env("APPSTRUCT_AUDIT_REDACT_FIELDS", r#"{"*":["/email"],"app::Project":["/secret"]}"#)
        .env("APPSTRUCT_AUDIT_RETENTION_DAYS", "30")
        .output().unwrap();
    assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
#[ignore = "runs in its own process with explicit audit policy"]
async fn configured_lifecycle() {
    let Some((root, db, schema)) = database().await else { return; };
    audit::validate_env().unwrap();
    let mail = MailState::default();
    let context = RequestContext::connection(&db, &mail, None, None);
    let snapshot = serde_json::json!({"email":"private@example.test","secret":"private","name":"keep"});
    audit::record(&db, &context, "app::Project", "1".into(), "create", None, Some(&snapshot)).await.unwrap();
    let event = db.fetch_optional(Sql::new("SELECT after FROM _appstruct_audit_events")).await.unwrap().unwrap();
    let value: serde_json::Value = event.try_get("after").unwrap();
    assert_eq!(value, serde_json::json!({"email":"[REDACTED]","secret":"[REDACTED]","name":"keep"}));
    let tx = db.begin().await.unwrap();
    audit::record(&tx, &context, "app::Project", "rollback".into(), "create", None, Some(&snapshot)).await.unwrap();
    tx.rollback().await.unwrap();
    let count: i64 = db.fetch_optional(Sql::new("SELECT COUNT(*) AS n FROM _appstruct_audit_events")).await.unwrap().unwrap().try_get("n").unwrap();
    assert_eq!(count, 1);
    db.execute_unprepared("INSERT INTO _appstruct_audit_events (id,entity,record_id,operation,occurred_at) SELECT gen_random_uuid(),'app::Project','old','create',CURRENT_TIMESTAMP - INTERVAL '90 days' FROM generate_series(1,3)").await.unwrap();
    assert_eq!(audit::prune_expired(&db, 2).await.unwrap(), 2);
    assert_eq!(audit::prune_expired(&db, 2).await.unwrap(), 1);
    assert_eq!(audit::prune_expired(&db, 2).await.unwrap(), 0);
    assert!(audit::prune_expired(&db, 0).await.is_err());

    let storage = Arc::new(Storage::default());
    let files = FileState::with_provider(db.clone(), storage.clone(), vec!["text/plain".into()]);
    for key in ["live", "orphan", "fresh", "rollback"] {
        files.put(key, "test.txt", "text/plain", b"hello", None).await.unwrap();
    }
    files.retain("live", "app::Project", "1", None).await.unwrap();
    files.retain("live", "app::Project", "1", None).await.unwrap();
    assert!(files.delete("live", None).await.is_err());
    assert!(files.retain("live", "app::Project", "2", Some(uuid::Uuid::now_v7())).await.is_err());
    let tx = db.begin().await.unwrap();
    let realtime = RealtimeState::default();
    let ctx = RequestContext::transaction_with_file(&tx, &mail, &files, &realtime, None, None);
    ctx.retain_file("rollback", "app::Project", "1").await.unwrap();
    tx.rollback().await.unwrap();
    let owners: serde_json::Value = db.fetch_optional(Sql::new("SELECT reference_owners FROM _appstruct_files WHERE object_key='rollback'")).await.unwrap().unwrap().try_get("reference_owners").unwrap();
    assert_eq!(owners, serde_json::json!([]));
    db.execute_unprepared("UPDATE _appstruct_files SET unreferenced_at=CURRENT_TIMESTAMP-INTERVAL '90 days' WHERE object_key IN ('orphan','live')").await.unwrap();
    storage.fail_delete.store(true, Ordering::SeqCst);
    assert!(files.collect_unreferenced(30, 1).await.is_err());
    assert!(files.retain("orphan", "app::Project", "1", None).await.is_err());
    storage.fail_delete.store(false, Ordering::SeqCst);
    assert_eq!(files.collect_unreferenced(30, 1).await.unwrap(), 1);
    assert!(files.get("orphan", None).await.is_err());
    assert!(files.get("live", None).await.is_ok());
    assert!(files.get("fresh", None).await.is_ok());
    files.release("live", "app::Project", "1", None).await.unwrap();
    files.release("live", "app::Project", "1", None).await.unwrap();
    assert_eq!(files.collect_unreferenced(30, 100).await.unwrap(), 0);
    db.execute_unprepared("UPDATE _appstruct_files SET unreferenced_at=CURRENT_TIMESTAMP-INTERVAL '90 days' WHERE object_key='live'").await.unwrap();
    assert_eq!(files.collect_unreferenced(30, 100).await.unwrap(), 1);
    assert!(files.collect_unreferenced(0, 100).await.is_err());
    files.put("race", "race.txt", "text/plain", b"race", None).await.unwrap();
    db.execute_unprepared("UPDATE _appstruct_files SET unreferenced_at=CURRENT_TIMESTAMP-INTERVAL '90 days' WHERE object_key='race'").await.unwrap();
    let (retained, collected) = tokio::join!(files.retain("race", "app::Project", "1", None), files.collect_unreferenced(30, 100));
    if retained.is_ok() {
        assert_eq!(collected.unwrap(), 0);
        assert!(files.get("race", None).await.is_ok());
    } else {
        assert_eq!(collected.unwrap(), 1);
        assert!(files.get("race", None).await.is_err());
    }
    files.put("serialized", "serialized.txt", "text/plain", b"serialized", None).await.unwrap();
    db.execute_unprepared("UPDATE _appstruct_files SET unreferenced_at=CURRENT_TIMESTAMP-INTERVAL '90 days' WHERE object_key='serialized'").await.unwrap();
    storage.pause_delete.store(true, Ordering::SeqCst);
    let worker_files = files.clone();
    let worker = tokio::spawn(async move { worker_files.collect_unreferenced(30, 100).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), storage.delete_entered.notified()).await.unwrap();
    assert_eq!(files.collect_unreferenced(30, 100).await.unwrap(), 0, "concurrent collection must skip a deletion still touching storage");
    storage.delete_resume.notify_one();
    assert_eq!(worker.await.unwrap().unwrap(), 1);
    let attachment = files.put("attachment", "test.txt", "text/plain", b"attachment", None).await.unwrap();
    let report = files.put("report", "report.txt", "text/plain", b"report", None).await.unwrap();
    db.execute(Sql::bind("INSERT INTO _appstruct_activity_entries (id,resource,record_id,kind,body,attachment_file_id,occurred_at) VALUES (gen_random_uuid(),'orders','1','comment','keep',$1,CURRENT_TIMESTAMP)", [attachment.id.into()])).await.unwrap();
    db.execute_unprepared("INSERT INTO _appstruct_report_templates (id,name,version,document_type,body,artifact_digest,input_schema,data_schema_version,renderer_version,created_at) VALUES ('00000000-0000-4000-8000-000000000001','test',1,'pdf','test','test','{}',1,'test',CURRENT_TIMESTAMP)").await.unwrap();
    db.execute(Sql::bind("INSERT INTO _appstruct_report_runs (id,template_id,template_version,idempotency_scope,idempotency_key,request_digest,snapshot_ciphertext,snapshot_digest,snapshot_size,locale,timezone,paper,orientation,stage,progress,result_file_id,created_at,expires_at) VALUES (gen_random_uuid(),'00000000-0000-4000-8000-000000000001',1,'test','test','test','test','test',1,'en','UTC','a4','portrait','succeeded',100,$1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP+INTERVAL '1 day')", [report.id.into()])).await.unwrap();
    db.execute_unprepared("UPDATE _appstruct_files SET unreferenced_at=CURRENT_TIMESTAMP-INTERVAL '90 days' WHERE object_key IN ('attachment','report')").await.unwrap();
    assert_eq!(files.collect_unreferenced(30, 100).await.unwrap(), 0);
    assert!(files.get("attachment", None).await.is_ok());
    assert!(files.get("report", None).await.is_ok());
    db.execute_unprepared("DELETE FROM _appstruct_activity_entries; DELETE FROM _appstruct_report_runs;").await.unwrap();
    assert_eq!(files.collect_unreferenced(30, 100).await.unwrap(), 2);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

fn report_options() -> ReconcileOptions { ReconcileOptions::default() }

#[tokio::test]
async fn storage_reconciliation_reports_and_deletes_only_old_orphans() {
    let Some((root, db, schema)) = database().await else { return; };
    let storage = Arc::new(Storage::default());
    let files = FileState::with_provider(db.clone(), storage.clone(), vec!["text/plain".into()]);

    // A tracked object: it has a metadata row, so it is never an orphan even when old.
    files.put("tracked", "t.txt", "text/plain", b"hello", None).await.unwrap();
    storage.age("tracked", 72);
    // Orphans: objects in storage with no row.
    storage.plant("orphan/old-a", 72);
    storage.plant("orphan/old-b", 48);
    storage.plant("orphan/young", 0);
    // A name AppStruct would never create. Not ours to judge.
    storage.plant("foreign\\name", 72);

    let report = files.reconcile_storage(report_options()).await.unwrap();
    assert_eq!(report.scanned, 5);
    assert_eq!(report.too_young, 1, "the fresh upload may still be committing");
    assert_eq!(report.foreign_keys, 1);
    assert_eq!(report.orphans, 2);
    assert_eq!((report.deleted, report.delete_failures), (0, 0), "report mode must not delete");
    let mut listed = report.orphan_keys.clone();
    listed.sort();
    assert_eq!(listed, ["orphan/old-a", "orphan/old-b"]);
    assert!(storage.get("orphan/old-a").await.is_ok(), "report mode must leave storage untouched");

    // Counts stay exact even when the listing is truncated.
    let truncated = files.reconcile_storage(ReconcileOptions { max_listed: 1, ..report_options() }).await.unwrap();
    assert_eq!((truncated.orphans, truncated.orphan_keys.len()), (2, 1));

    // Option validation.
    for bad in [
        ReconcileOptions { min_age: std::time::Duration::from_secs(60), ..report_options() },
        ReconcileOptions { min_age: std::time::Duration::from_secs(10_000_000_000_000), ..report_options() },
        ReconcileOptions { min_age: std::time::Duration::MAX, ..report_options() },
        ReconcileOptions { batch_size: 0, ..report_options() },
        ReconcileOptions { batch_size: 1001, ..report_options() },
        ReconcileOptions { max_listed: 1001, ..report_options() },
        ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 0, ..report_options() },
        ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 10001, ..report_options() },
    ] {
        assert!(matches!(files.reconcile_storage(bad).await, Err(FileError::Configuration(_))));
    }

    // Delete mode removes only old orphans and leaves everything else.
    let deleted = files.reconcile_storage(ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 10, ..report_options() }).await.unwrap();
    assert_eq!((deleted.orphans, deleted.deleted, deleted.delete_failures), (2, 2, 0));
    assert!(storage.get("orphan/old-a").await.is_err());
    assert!(storage.get("orphan/old-b").await.is_err());
    for kept in ["tracked", "orphan/young", "foreign\\name"] {
        assert!(storage.get(kept).await.is_ok(), "{kept} must survive");
    }
    assert!(files.get("tracked", None).await.is_ok());

    // max_deletes caps work per pass but the orphan count stays exact. Small batches exercise
    // more than one database lookup per pass.
    for key in ["cap/a", "cap/b", "cap/c"] { storage.plant(key, 72); }
    let capped = files.reconcile_storage(ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 2, batch_size: 1, ..report_options() }).await.unwrap();
    assert_eq!((capped.orphans, capped.deleted), (3, 2));
    let remaining = files.reconcile_storage(report_options()).await.unwrap();
    assert_eq!(remaining.orphans, 1);

    // A failing delete is counted, not fatal, and does not end the pass.
    storage.fail_delete.store(true, Ordering::SeqCst);
    let failed = files.reconcile_storage(ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 5, ..report_options() }).await.unwrap();
    assert_eq!((failed.orphans, failed.deleted, failed.delete_failures), (1, 0, 1));
    storage.fail_delete.store(false, Ordering::SeqCst);

    // Clear the orphan left by the capped pass so the race below is the only work.
    let cleared = files.reconcile_storage(ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 5, ..report_options() }).await.unwrap();
    assert_eq!((cleared.orphans, cleared.deleted), (1, 1));

    // A row that commits after the batch lookup but before the delete must save its object.
    // Both keys share one batch, so both are classified as orphans before any delete starts.
    // Pausing the first delete lets the test commit a row for the second key in that window.
    storage.plant("race/a", 72);
    storage.plant("race/b", 72);
    storage.pause_delete.store(true, Ordering::SeqCst);
    let racer = files.clone();
    let pass = tokio::spawn(async move {
        racer.reconcile_storage(ReconcileOptions { mode: ReconcileMode::Delete, max_deletes: 5, ..report_options() }).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), storage.delete_entered.notified()).await.unwrap();
    db.execute_unprepared("INSERT INTO _appstruct_files (id, object_key, original_name, content_type, size, checksum, created_at) VALUES (gen_random_uuid(), 'race/b', 'b.txt', 'text/plain', 1, 'x', CURRENT_TIMESTAMP)").await.unwrap();
    storage.delete_resume.notify_one();
    let raced = pass.await.unwrap().unwrap();
    assert_eq!((raced.orphans, raced.deleted), (2, 1), "race/b was classified as an orphan but must not be deleted");
    assert!(storage.get("race/a").await.is_err());
    assert!(storage.get("race/b").await.is_ok(), "an object whose row committed late must survive");

    // A provider that cannot list reports Unsupported and changes nothing.
    let unsupported = FileState::with_provider(db.clone(), Arc::new(NoList), vec![]);
    assert!(matches!(unsupported.reconcile_storage(report_options()).await, Err(FileError::Unsupported)));

    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}
