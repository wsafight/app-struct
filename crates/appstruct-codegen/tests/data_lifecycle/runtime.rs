use appstruct_generated_backend::{audit, FileError, FileProvider, FileState, MailState, RealtimeState, RequestContext, database::{Database, DatabaseExecutor, Sql}};
use sqlx::Row as _;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

#[derive(Default)]
struct Storage {
    data: Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
    fail_delete: AtomicBool,
    pause_delete: AtomicBool,
    delete_entered: tokio::sync::Notify,
    delete_resume: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl FileProvider for Storage {
    fn name(&self) -> &'static str { "test" }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), FileError> {
        self.data.lock().unwrap().insert(key.into(), bytes.into()); Ok(())
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
        self.data.lock().unwrap().remove(key); Ok(())
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
