use appstruct_generated_backend::{
    AppExtensions, AuthState, FileState, MailState,
    database::{Database, DatabaseExecutor, Sql},
    router_with_services, run_product_job,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use serde_json::{Value, json};
use sqlx::Row as _;
use tower::ServiceExt as _;

#[path = "product/capabilities.rs"]
mod capabilities;

#[derive(Clone)]
struct Session {
    cookies: String,
    csrf: String,
    tenant: String,
}

async fn request(
    router: &Router,
    session: &Session,
    method: &str,
    path: &str,
    body: Value,
    key: &str,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &session.cookies)
        .header("x-csrf-token", &session.csrf);
    if !session.tenant.is_empty() {
        request = request.header("x-appstruct-tenant", &session.tenant);
    }
    if !key.is_empty() {
        request = request.header("Idempotency-Key", key);
    }
    router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn allowance_is_tenant_scoped_atomic_idempotent_and_refundable() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else {
        return;
    };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("product_{}", uuid::Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let database = Database::connect(format!("{url}{separator}options=-csearch_path%3D{schema}"))
        .await
        .unwrap();
    database
        .execute_unprepared(include_str!("../../database/0001_initial.sql"))
        .await
        .unwrap();
    let files = FileState::from_env(database.clone()).unwrap();
    let mail = MailState::from_env(database.clone()).unwrap();
    let router = router_with_services(
        database.clone(),
        AppExtensions::builder().build(),
        AuthState::from_env().unwrap(),
        mail,
        files.clone(),
    );
    let mut session = Session {
        cookies: String::new(),
        csrf: String::new(),
        tenant: String::new(),
    };
    assert_eq!(
        request(
            &router,
            &session,
            "GET",
            "/api/admin/business",
            json!({}),
            ""
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = request(
        &router,
        &session,
        "POST",
        "/api/auth/register",
        json!({"email":"product@example.test","password":"Product-Report-Password-2026"}),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|item| item.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>();
    session.csrf = cookies
        .iter()
        .find_map(|item| item.strip_prefix("appstruct_csrf="))
        .unwrap()
        .to_owned();
    session.cookies = cookies.join("; ");
    assert_eq!(
        request(
            &router,
            &session,
            "GET",
            "/api/admin/business",
            json!({}),
            ""
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let response = request(
        &router,
        &session,
        "POST",
        "/api/tenant/organizations",
        json!({"name":"Reports team"}),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    session.tenant = json_body(response).await["id"].as_str().unwrap().to_owned();
    let upload = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/files?name=team.txt")
                .header(header::CONTENT_TYPE, "text/plain")
                .header(header::COOKIE, &session.cookies)
                .header("x-csrf-token", &session.csrf)
                .header("x-appstruct-tenant", &session.tenant)
                .body(Body::from("Team input data"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::CREATED);
    let uploaded = json_body(upload).await;
    let file_id = uploaded["id"].as_str().unwrap();
    let object_key = uploaded["object_key"].as_str().unwrap();
    let listing =
        json_body(request(&router, &session, "GET", "/api/files", json!({}), "").await).await;
    assert_eq!(listing["meta"]["total"], 1);
    let downloaded = request(
        &router,
        &session,
        "GET",
        &format!("/api/files/{file_id}/download"),
        json!({}),
        "",
    )
    .await;
    assert_eq!(downloaded.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(downloaded.into_body(), 1024)
            .await
            .unwrap()
            .as_ref(),
        b"Team input data"
    );
    let second = request(
        &router,
        &session,
        "POST",
        "/api/tenant/organizations",
        json!({"name":"Other team"}),
        "",
    )
    .await;
    let mut second_session = session.clone();
    second_session.tenant = json_body(second).await["id"].as_str().unwrap().to_owned();
    let denied = request(
        &router,
        &second_session,
        "GET",
        &format!("/api/files/{file_id}/download"),
        json!({}),
        "",
    )
    .await;
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);
    files
        .retain(
            object_key,
            "app::Project",
            "protected",
            Some(uuid::Uuid::parse_str(&session.tenant).unwrap()),
        )
        .await
        .unwrap();
    let protected = request(
        &router,
        &session,
        "DELETE",
        &format!("/api/files/{file_id}"),
        json!({}),
        "",
    )
    .await;
    assert_eq!(protected.status(), StatusCode::CONFLICT);
    files
        .release(
            object_key,
            "app::Project",
            "protected",
            Some(uuid::Uuid::parse_str(&session.tenant).unwrap()),
        )
        .await
        .unwrap();
    let removed = request(
        &router,
        &session,
        "DELETE",
        &format!("/api/files/{file_id}"),
        json!({}),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let data = json!({"data":{"title":"Team report","summary":"A useful summary"}});
    let mut pending = tokio::task::JoinSet::new();
    for index in 0..12 {
        let (router, session, data) = (router.clone(), session.clone(), data.clone());
        pending.spawn(async move {
            let key = format!("report-{index}");
            let response = request(
                &router,
                &session,
                "POST",
                "/api/reports/templates/team-summary/runs",
                data,
                &key,
            )
            .await;
            (key, response.status(), json_body(response).await)
        });
    }
    let mut accepted = Vec::new();
    while let Some(result) = pending.join_next().await {
        let (key, status, body) = result.unwrap();
        match status {
            StatusCode::ACCEPTED => accepted.push((key, body["id"].as_str().unwrap().to_owned())),
            StatusCode::PAYMENT_REQUIRED => assert_eq!(body["error"]["code"], "QUOTA_EXCEEDED"),
            other => panic!("unexpected {other}: {body}"),
        }
    }
    assert_eq!(accepted.len(), 3);
    let response = request(
        &router,
        &session,
        "POST",
        "/api/reports/templates/team-summary/runs",
        data.clone(),
        &accepted[0].0,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["id"], accepted[0].1);
    let response = request(
        &router,
        &session,
        "POST",
        &format!("/api/reports/runs/{}/cancel", accepted[0].1),
        json!({}),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &router,
        &session,
        "POST",
        &format!("/api/reports/runs/{}/retry", accepted[0].1),
        json!({}),
        "after-cancel",
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let broken = accepted[1].1.clone();
    database
        .execute(Sql::bind(
            "UPDATE _appstruct_report_runs SET snapshot_ciphertext = 'broken' WHERE id = $1",
            [uuid::Uuid::parse_str(&broken).unwrap().into()],
        ))
        .await
        .unwrap();
    for _ in 0..4 {
        run_product_job(database.clone(), files.clone()).await;
    }
    let row = database
        .fetch_optional(Sql::bind(
            "SELECT refunded FROM _appstruct_billing_usage WHERE id = $1",
            [uuid::Uuid::parse_str(&broken).unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(row.try_get::<bool, _>("refunded").unwrap());
    let response = request(
        &router,
        &session,
        "POST",
        "/api/reports/templates/team-summary/runs",
        data.clone(),
        "after-failure",
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let organization = uuid::Uuid::parse_str(&session.tenant).unwrap();
    database.execute(Sql::bind("INSERT INTO _appstruct_billing_subscriptions (id,organization_id,provider_subscription_id,plan_id,status,current_period_end,cancel_at_period_end,created_at,updated_at) VALUES ($1,$2,'sub_product','pro','active',CURRENT_TIMESTAMP + INTERVAL '1 day',false,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)", [uuid::Uuid::now_v7().into(), organization.into()])).await.unwrap();
    let response = request(
        &router,
        &session,
        "POST",
        "/api/reports/templates/team-summary/runs",
        data.clone(),
        "paid",
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    database
        .execute_unprepared(
            "UPDATE _appstruct_auth_accounts SET roles = '[\"member\",\"admin\"]'::jsonb",
        )
        .await
        .unwrap();
    let mut admin = session.clone();
    admin.tenant.clear();
    let response = request(&router, &admin, "GET", "/api/admin/business", json!({}), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let overview = json_body(response).await;
    assert!(overview["generated_at"].is_string());
    let metrics = overview["metrics"].as_array().unwrap();
    let metric = |key: &str| metrics.iter().find(|metric| metric["key"] == key).unwrap();
    assert_eq!(metric("registered_accounts")["value"], 1);
    assert_eq!(metric("organizations")["value"], 2);
    assert_eq!(metric("paying_organizations")["value"], 1);
    assert!(metric("successful_reports")["value"].as_u64().unwrap() > 0);
    assert_eq!(metric("activated_accounts")["value"], 1);
    let successful = database
        .fetch_optional(Sql::new(
            "SELECT id FROM _appstruct_report_runs WHERE stage = 'succeeded' LIMIT 1".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<uuid::Uuid, _>("id")
        .unwrap();
    let download = request(
        &router,
        &session,
        "GET",
        &format!("/api/reports/runs/{successful}/download"),
        json!({}),
        "",
    )
    .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert!(
        to_bytes(download.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .starts_with(b"%PDF-")
    );
    assert_eq!(metric("visits")["value"], 0);
    assert!(metric("revenue")["value"].is_null());
    assert!(metric("revenue")["unavailable_reason"].is_string());
    let mut other = session.clone();
    other.tenant = uuid::Uuid::now_v7().to_string();
    let response = request(
        &router,
        &other,
        "POST",
        "/api/reports/templates/team-summary/runs",
        data.clone(),
        "other-tenant",
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    database.execute_unprepared("UPDATE _appstruct_billing_subscriptions SET current_period_end = CURRENT_TIMESTAMP - INTERVAL '1 day'").await.unwrap();
    let response = request(
        &router,
        &session,
        "POST",
        "/api/reports/templates/team-summary/runs",
        data.clone(),
        "expired-plan",
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    database
        .execute_unprepared("UPDATE _appstruct_billing_subscriptions SET status = 'canceled'")
        .await
        .unwrap();
    let response = request(
        &router,
        &session,
        "POST",
        "/api/reports/templates/team-summary/runs",
        data,
        "canceled-plan",
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    let overview =
        json_body(request(&router, &session, "GET", "/api/billing", json!({}), "").await).await;
    assert_eq!(overview["usage"][0]["used"], 3);
    assert_eq!(overview["usage"][0]["remaining"], 0);
    assert_eq!(overview["usage"][0]["paid"], false);
    assert!(overview["entitlements"].as_array().unwrap().is_empty());
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
