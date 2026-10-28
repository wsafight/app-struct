use appstruct_generated_backend::{AppExtensions, AuthState, database::{Database, DatabaseExecutor, Sql}, router_with_auth};
use axum::{Router, body::{Body, to_bytes}, http::{Request, StatusCode, header}};
use sqlx::Row as _;
use tower::ServiceExt as _;

async fn post(router: &Router, path: &str, body: &str, cookies: &str, csrf: &str) -> axum::response::Response {
    router.clone().oneshot(Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, cookies).header("x-csrf-token", csrf)
        .body(Body::from(body.to_owned())).unwrap()).await.unwrap()
}

fn limited(response: &axum::response::Response) {
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()[header::RETRY_AFTER], "60");
}

#[tokio::test]
async fn limits_persist_across_routers_and_reset_after_the_window() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else { return; };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("auth_limits_{}", uuid::Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let scoped_url = format!("{url}{separator}options=-csearch_path%3D{schema}");
    let database = Database::connect(&scoped_url).await.unwrap();
    database.execute_unprepared(include_str!("../../database/0001_initial.sql")).await.unwrap();
    let routers = [
        router_with_auth(database.clone(), AppExtensions::builder().build(), AuthState::from_env().unwrap()).unwrap(),
        router_with_auth(Database::connect(&scoped_url).await.unwrap(), AppExtensions::builder().build(), AuthState::from_env().unwrap()).unwrap(),
    ];
    let credentials = r#"{"email":"LIMITS@example.test","password":"AppStruct-Rate-Limits-2026"}"#;
    let registered = post(&routers[0], "/api/auth/register", credentials, "", "").await;
    assert_eq!(registered.status(), StatusCode::OK);
    let cookies = registered.headers().get_all(header::SET_COOKIE).iter()
        .map(|value| value.to_str().unwrap().split(';').next().unwrap().to_owned()).collect::<Vec<_>>();
    let csrf = cookies.iter().find_map(|value| value.strip_prefix("appstruct_csrf=")).unwrap().to_owned();
    let cookies = cookies.join("; ");
    let response: serde_json::Value = serde_json::from_slice(&to_bytes(registered.into_body(), 8192).await.unwrap()).unwrap();
    let user_id = response["user"]["id"].as_str().unwrap();

    for attempt in 1..10 {
        assert_eq!(post(&routers[attempt % 2], "/api/auth/register", credentials, "", "").await.status(), StatusCode::CONFLICT);
    }
    limited(&post(&routers[1], "/api/auth/register", credentials, "", "").await);
    assert_eq!(post(&routers[0], "/api/auth/login", credentials, "", "").await.status(), StatusCode::OK);
    limited(&post(&routers[0], "/api/auth/register", credentials, "", "").await);

    for attempt in 1..10 {
        assert_eq!(post(&routers[attempt % 2], "/api/auth/email/request", "{}", &cookies, &csrf).await.status(), StatusCode::NO_CONTENT);
    }
    limited(&post(&routers[1], "/api/auth/email/request", "{}", &cookies, &csrf).await);
    let captured = database.fetch_optional(Sql::new("SELECT count(*) AS count FROM _appstruct_auth_mail_capture WHERE subject = 'Verify your email address'")).await.unwrap().unwrap();
    assert_eq!(captured.try_get::<i64, _>("count").unwrap(), 10);

    let reset = r#"{"email":"limits@example.test"}"#;
    let mut requests = tokio::task::JoinSet::new();
    for attempt in 0..20 {
        let router = routers[attempt % 2].clone();
        requests.spawn(async move { post(&router, "/api/auth/password/request", reset, "", "").await.status() });
    }
    let mut allowed = 0;
    let mut denied = 0;
    while let Some(status) = requests.join_next().await {
        match status.unwrap() {
            StatusCode::NO_CONTENT => allowed += 1,
            StatusCode::TOO_MANY_REQUESTS => denied += 1,
            status => panic!("unexpected reset status: {status}"),
        }
    }
    assert_eq!((allowed, denied), (10, 10));
    limited(&post(&routers[0], "/api/auth/password/request", reset, "", "").await);
    database.execute(Sql::bind("UPDATE _appstruct_auth_login_attempts SET window_started_at = CURRENT_TIMESTAMP - INTERVAL '61 seconds' WHERE key IN ($1, $2, $3)",
        ["register:limits@example.test".into(), "reset:limits@example.test".into(), format!("verify:{user_id}").into()])).await.unwrap();
    assert_eq!(post(&routers[1], "/api/auth/register", credentials, "", "").await.status(), StatusCode::CONFLICT);
    assert_eq!(post(&routers[0], "/api/auth/password/request", reset, "", "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(post(&routers[1], "/api/auth/email/request", "{}", &cookies, &csrf).await.status(), StatusCode::NO_CONTENT);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}
