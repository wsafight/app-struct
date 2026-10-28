use appstruct_generated_backend::{ApiError, AppExtensions, RequestContext, api::appointment, database::{Database, DatabaseExecutor, Sql}, extensions::AppointmentHooks, router};
use async_trait::async_trait;
use axum::{Router, body::{Body, to_bytes}, http::{Request, StatusCode}};
use tower::ServiceExt as _;
use sqlx::Row as _;

struct Rules;
#[async_trait]
impl AppointmentHooks for Rules {
    async fn before_create(&self, ctx: &RequestContext, input: &mut appointment::CreateInput) -> Result<(), ApiError> {
        if input.note.as_deref() == Some("hook-invalid") {
            ctx.database().execute(Sql::new("INSERT INTO hook_marks DEFAULT VALUES")).await?;
            input.ends = Some(input.starts);
        }
        Ok(())
    }
}
async fn request(router: &Router, method: &str, path: &str, body: serde_json::Value, revision: Option<i64>) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder().method(method).uri(path).header("content-type", "application/json");
    if let Some(revision) = revision { request = request.header("if-match", format!("\"rev-{revision}\"")); }
    let response = router.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn input() -> serde_json::Value {
    serde_json::json!({"needs_note": false, "note": "reason", "count": 0, "starts": "2026-10-01T00:00:00.000001Z", "ends": "2026-10-01T00:00:00.000002Z", "minimum": "-1"})
}
#[tokio::test]
async fn rejects_invalid_create_patch_bulk_workflow_and_hook_candidates_atomically() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else { return; };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("form_rules_{}", uuid::Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let database = Database::connect(&format!("{url}{separator}options=-csearch_path%3D{schema}")).await.unwrap();
    database.execute_unprepared(include_str!("../../database/0001_initial.sql")).await.unwrap();
    database.execute_unprepared("CREATE TABLE hook_marks (id integer GENERATED ALWAYS AS IDENTITY)").await.unwrap();
    let router = router(database.clone(), AppExtensions::builder().appointment_hooks(Rules).build()).unwrap();
    let mut invalid = input(); invalid["note"] = serde_json::json!("");
    let (status, body) = request(&router, "POST", "/api/appointments/", invalid, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["fields"][0]["field"], "note");
    let mut invalid = input(); invalid["note"] = serde_json::json!("hook-invalid");
    assert_eq!(request(&router, "POST", "/api/appointments/", invalid, None).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, model) = request(&router, "POST", "/api/appointments/", input(), None).await;
    assert_eq!(status, StatusCode::CREATED, "{model}");
    assert_eq!(model["count"], 0); // Zero is present; the false condition is active.
    let id = model["id"].as_str().unwrap(); let path = format!("/api/appointments/{id}");
    let (status, body) = request(&router, "PATCH", &path, serde_json::json!({"starts":"2026-10-01T00:00:00.000003Z"}), Some(1)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(request(&router, "PATCH", &path, serde_json::json!({"count":null}), Some(1)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = request(&router, "GET", &path, serde_json::Value::Null, None).await;
    assert_eq!(status, StatusCode::OK); assert_eq!(body["revision"], 1); assert_eq!(body["starts"], model["starts"]);
    let (status, body) = request(&router, "PATCH", "/api/appointments/_bulk", serde_json::json!({"ids":[id],"expected_revisions":{id:1},"patch":{"ends":"2026-10-01T00:00:00.000001Z"}}), None).await;
    assert_eq!(status, StatusCode::OK, "{body}"); assert_eq!(body["failed"].as_array().unwrap().len(), 1);
    let transition = format!("{path}/_transitions/publish");
    let (status, body) = request(&router, "POST", &transition, serde_json::json!({}), Some(1)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = request(&router, "PATCH", &path, serde_json::json!({"amount":"0"}), Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(request(&router, "POST", &transition, serde_json::json!({}), Some(2)).await.0, StatusCode::OK);
    // Exact decimal comparisons must not collapse values beyond JS's safe integer range.
    let (status, body) = request(&router, "PATCH", &path, serde_json::json!({"minimum":"9007199254740993.14","amount":"9007199254740993.15"}), Some(3)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(request(&router, "PATCH", &path, serde_json::json!({"minimum":"9007199254740993.16"}), Some(4)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = request(&router, "PATCH", &path, serde_json::json!({"needs_note":true, "count":null, "note":null}), Some(4)).await;
    assert_eq!(status, StatusCode::OK, "{body}"); assert!(body["count"].is_null()); assert!(body["note"].is_null());
    let batch = uuid::Uuid::now_v7().to_string();
    let response = router.clone().oneshot(Request::builder().method("POST").uri("/api/appointments/_import.csv")
        .header("content-type", "text/csv").header("idempotency-key", batch)
        .body(Body::from("needs_note,note,count,starts,ends,minimum\nfalse,,0,2026-01-01T00:00:00Z,2026-01-02T00:00:00Z,-1\nfalse,ok,0,2026-01-01T00:00:00Z,2026-01-02T00:00:00Z,-1\n")).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();
    assert_eq!(result["succeeded"], serde_json::json!(["1"])); assert_eq!(result["failed"][0]["id"], "0");
    let marks = database.fetch_optional(Sql::new("SELECT count(*) AS count FROM hook_marks")).await.unwrap().unwrap();
    assert_eq!(marks.try_get::<i64, _>("count").unwrap(), 0);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}
