use appstruct_generated_backend::{AppExtensions, AuthState, database::{Database, DatabaseExecutor, Sql}, router_with_auth};
use axum::{Router, body::{Body, to_bytes}, http::{Request, StatusCode, header}};
use sqlx::Row as _;
use tower::ServiceExt as _;

#[derive(Default)]
struct Session { cookies: String, csrf: String, tenant: String }

async fn request(router: &Router, session: &Session, method: &str, path: &str, body: &str, batch: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder().method(method).uri(path)
        .header(header::CONTENT_TYPE, if path.ends_with(".csv") { "text/csv" } else { "application/json" })
        .header(header::COOKIE, &session.cookies).header("x-csrf-token", &session.csrf);
    if !session.tenant.is_empty() { request = request.header("x-appstruct-tenant", &session.tenant); }
    if let Some(batch) = batch { request = request.header("idempotency-key", batch); }
    router.clone().oneshot(request.body(Body::from(body.to_owned())).unwrap()).await.unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap()
}

async fn register(router: &Router, email: &str) -> Session {
    let response = request(router, &Session::default(), "POST", "/api/auth/register", &format!(r#"{{"email":"{email}","password":"AppStruct-import-2026"}}"#), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookies = response.headers().get_all(header::SET_COOKIE).iter().map(|value| value.to_str().unwrap().split(';').next().unwrap().to_owned()).collect::<Vec<_>>();
    let csrf = cookies.iter().find_map(|value| value.strip_prefix("appstruct_csrf=")).unwrap().to_owned();
    Session { cookies: cookies.join("; "), csrf, tenant: String::new() }
}

#[tokio::test]
async fn replays_concurrent_lost_responses_and_preserves_scope_and_atomicity() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else { return; };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("import_batches_{}", uuid::Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let scoped = format!("{url}{separator}options=-csearch_path%3D{schema}");
    let database = Database::connect(&scoped).await.unwrap();
    database.execute_unprepared(include_str!("../../database/0001_initial.sql")).await.unwrap();
    let router = router_with_auth(database.clone(), AppExtensions::builder().build(), AuthState::from_env().unwrap()).unwrap();
    let mut session = register(&router, "imports@example.test").await;
    let organization = request(&router, &session, "POST", "/api/tenant/organizations", r#"{"name":"Import Alpha"}"#, None).await;
    assert_eq!(organization.status(), StatusCode::CREATED);
    session.tenant = json(organization).await["id"].as_str().unwrap().to_owned();
    let batch = uuid::Uuid::now_v7().to_string();
    let csv = "code,title,secret\nfirst,Keep First,private\nsecond,Keep Second,private\n";
    assert_eq!(request(&router, &session, "POST", "/api/entries/_import.csv", csv, None).await.status(), StatusCode::BAD_REQUEST);
    let (first, replay) = tokio::join!(
        request(&router, &session, "POST", "/api/entries/_import.csv", csv, Some(&batch)),
        request(&router, &session, "POST", "/api/entries/_import.csv", csv, Some(&batch))
    );
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(replay.status(), StatusCode::OK);
    let first = json(first).await;
    assert_eq!(first["succeeded"], serde_json::json!(["0", "1"]));
    assert_eq!(first, json(replay).await);
    // Ignore the POST response, as if the network lost it; recover using the batch identifier.
    let result = request(&router, &session, "GET", &format!("/api/entries/_imports/{batch}"), "", None).await;
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(first, json(result).await);
    assert_eq!(request(&router, &session, "POST", "/api/entries/_import.csv", "code,title,secret\nthird,Changed,private\n", Some(&batch)).await.status(), StatusCode::CONFLICT);
    let count = database.fetch_optional(Sql::new("SELECT count(*) AS count FROM entries")).await.unwrap().unwrap();
    assert_eq!(count.try_get::<i64, _>("count").unwrap(), 2);
    let outsider = register(&router, "outsider@example.test").await;
    assert_eq!(request(&router, &outsider, "GET", &format!("/api/entries/_imports/{batch}"), "", None).await.status(), StatusCode::NOT_FOUND);
    let other = request(&router, &session, "POST", "/api/tenant/organizations", r#"{"name":"Import Beta"}"#, None).await;
    session.tenant = json(other).await["id"].as_str().unwrap().to_owned();
    assert_eq!(request(&router, &session, "GET", &format!("/api/entries/_imports/{batch}"), "", None).await.status(), StatusCode::NOT_FOUND);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}
