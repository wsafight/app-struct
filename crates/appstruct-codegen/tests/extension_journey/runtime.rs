mod example;
use appstruct_generated_backend::{database::{Database, DatabaseExecutor, Sql}, router};
use axum::{body::Body, http::{Request, StatusCode}};
use sqlx::Row as _;
use std::{io::Write, sync::{Arc, Mutex}};
use tower::ServiceExt as _;

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);
impl Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> { self.0.lock().unwrap().extend_from_slice(bytes); Ok(bytes.len()) }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[tokio::test]
async fn policy_hook_rollback_and_after_commit_logs() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else { return; };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("extensions_{}", uuid::Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let db = Database::connect(format!("{url}{separator}options=-csearch_path%3D{schema}")).await.unwrap();
    db.execute_unprepared(include_str!("../../database/0001_initial.sql")).await.unwrap();
    let logs = Logs::default();
    let writer = logs.clone();
    tracing::subscriber::set_global_default(tracing_subscriber::fmt().json().with_writer(move || writer.clone()).finish()).unwrap();
    let api = router(db.clone(), example::extensions()).unwrap();
    for (code, expected, count) in [("DENY", StatusCode::FORBIDDEN, 0), ("FAIL", StatusCode::FORBIDDEN, 0), ("OK", StatusCode::CREATED, 1), ("COMMIT", StatusCode::CREATED, 2)] {
        let response = api.clone().oneshot(Request::post("/api/projects/").header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"code":code,"name":"Example project"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), expected);
        let request_id = response.headers().get("x-request-id").unwrap().to_str().unwrap().to_owned();
        assert!(uuid::Uuid::parse_str(&request_id).is_ok());
        for table in ["projects", "tasks"] {
            let n: i64 = db.fetch_optional(Sql::new(format!("SELECT COUNT(*) AS n FROM {table}"))).await.unwrap().unwrap().try_get("n").unwrap();
            assert_eq!(n, count, "{code} must preserve transaction semantics for {table}");
        }
        if code == "FAIL" || code == "COMMIT" {
            let raw = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
            let events = raw.lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()).collect::<Vec<_>>();
            let message = if code == "FAIL" { "demonstration failure" } else { "after_commit hook failed" };
            assert!(events.iter().any(|event| event["fields"]["message"] == message && event.to_string().contains(&request_id)), "hook log must correlate to {request_id}: {raw}");
        }
    }
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}
