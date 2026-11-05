use appstruct_generated_backend::{
    AuthState, FileState, MailState,
    database::{Database, DatabaseExecutor, Sql},
    router_with_services,
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
use uuid::Uuid;

#[derive(Clone, Default)]
pub(crate) struct Session {
    pub(crate) cookies: String,
    pub(crate) csrf: String,
    pub(crate) tenant: String,
}

pub(crate) async fn setup() -> Option<(Database, Database, String, Router, FileState)> {
    let url = std::env::var("APPSTRUCT_E2E_DATABASE_URL").ok()?;
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("collaboration_{}", Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let database = Database::connect(format!("{url}{separator}options=-csearch_path%3D{schema}"))
        .await
        .unwrap();
    database
        .execute_unprepared(include_str!("../../../generated/database/0001_initial.sql"))
        .await
        .unwrap();
    let files = FileState::from_env(database.clone()).unwrap();
    let router = router_with_services(
        database.clone(),
        crate::extensions(),
        AuthState::from_env().unwrap(),
        MailState::from_env(database.clone()).unwrap(),
        files.clone(),
    );
    Some((root, database, schema, router, files))
}

pub(crate) async fn request(
    router: &Router,
    session: &Session,
    method: &str,
    path: &str,
    value: Value,
    key: &str,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &session.cookies)
        .header("x-csrf-token", &session.csrf);
    if !session.tenant.is_empty() {
        builder = builder.header("x-appstruct-tenant", &session.tenant);
    }
    if !key.is_empty() {
        builder = builder.header("Idempotency-Key", key);
    }
    router
        .clone()
        .oneshot(builder.body(Body::from(value.to_string())).unwrap())
        .await
        .unwrap()
}

pub(crate) async fn body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

pub(crate) async fn user(router: &Router, email: &str) -> (Session, Uuid) {
    let mut session = Session::default();
    let response = request(
        router,
        &session,
        "POST",
        "/api/auth/register",
        json!({ "email": email, "password": "Collaboration-Secure-Password-2026" }),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| {
            value
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    session.csrf = cookies
        .iter()
        .find_map(|value| value.strip_prefix("appstruct_csrf="))
        .unwrap()
        .to_owned();
    session.cookies = cookies.join("; ");
    let id = Uuid::parse_str(body(response).await["user"]["id"].as_str().unwrap()).unwrap();
    (session, id)
}

pub(crate) async fn team(router: &Router, session: &mut Session) -> Uuid {
    let response = request(
        router,
        session,
        "POST",
        "/api/tenant/organizations",
        json!({ "name": "Collaboration team" }),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    session.tenant = body(response).await["id"].as_str().unwrap().to_owned();
    Uuid::parse_str(&session.tenant).unwrap()
}

#[tokio::test]
async fn approval_snapshot_decisions_evidence_and_tenant_access() {
    let Some((root, database, schema, router, files)) = setup().await else {
        return;
    };
    let (mut requester, actor) = user(&router, "requester@example.test").await;
    let tenant = team(&router, &mut requester).await;
    let (mut reviewer, reviewer_id) = user(&router, "reviewer@example.test").await;
    reviewer.tenant.clone_from(&requester.tenant);
    database.execute(Sql::bind("INSERT INTO \"_appstruct_tenant_memberships\" (organization_id, user_id, role, created_at) VALUES ($1, $2, 'member', CURRENT_TIMESTAMP)", [tenant.into(), reviewer_id.into()])).await.unwrap();
    let task = body(
        request(
            &router,
            &requester,
            "POST",
            "/api/tasks/",
            json!({ "title": "Review deliverable" }),
            "",
        )
        .await,
    )
    .await;
    let task_id = Uuid::parse_str(task["id"].as_str().unwrap()).unwrap();
    let report = request(
        &router,
        &requester,
        "POST",
        "/api/reports/templates/team-summary/runs",
        json!({ "data": { "title": "Evidence", "summary": "Review input" } }),
        "approval-report",
    )
    .await;
    assert_eq!(report.status(), StatusCode::ACCEPTED);
    let run_id = Uuid::parse_str(body(report).await["id"].as_str().unwrap()).unwrap();
    let key = format!("reports/{tenant}/{run_id}.pdf");
    let metadata = files
        .put(
            &key,
            "evidence.pdf",
            "application/pdf",
            b"%PDF-1.4\n%%EOF",
            Some(tenant),
        )
        .await
        .unwrap();
    database.execute(Sql::bind("UPDATE \"_appstruct_report_runs\" SET stage = 'succeeded', result_object_key = $2, result_file_id = $3 WHERE id = $1 AND actor_id = $4", [run_id.into(), key.into(), metadata.id.into(), actor.into()])).await.unwrap();
    let input = json!({ "task_id": task_id, "report_run_id": run_id, "reviewer_id": reviewer_id, "note": "Please review" });
    let approval_id = Uuid::now_v7();
    let response = request(
        &router,
        &requester,
        "POST",
        "/api/product/approvals",
        input.clone(),
        &approval_id.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        request(
            &router,
            &requester,
            "POST",
            "/api/product/approvals",
            input.clone(),
            &approval_id.to_string()
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        request(
            &router,
            &requester,
            "POST",
            "/api/product/approvals",
            input.clone(),
            &Uuid::now_v7().to_string()
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let decision_path = format!("/api/product/approvals/{approval_id}/decide");
    let decision = json!({ "decision": "approved", "note": "Approved" });
    assert_eq!(
        request(
            &router,
            &requester,
            "POST",
            &decision_path,
            decision.clone(),
            ""
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    database
        .execute(Sql::bind(
            "UPDATE tasks SET revision = revision + 1, title = 'Changed' WHERE id = $1",
            [task_id.into()],
        ))
        .await
        .unwrap();
    assert_eq!(
        request(
            &router,
            &reviewer,
            "POST",
            &decision_path,
            decision.clone(),
            ""
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &router,
            &requester,
            "POST",
            &decision_path,
            json!({ "decision": "cancelled", "note": "Changed task" }),
            ""
        )
        .await
        .status(),
        StatusCode::OK
    );
    let second_id = Uuid::now_v7();
    assert_eq!(
        request(
            &router,
            &requester,
            "POST",
            "/api/product/approvals",
            input,
            &second_id.to_string()
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let decision_path = format!("/api/product/approvals/{second_id}/decide");
    for _ in 0..2 {
        assert_eq!(
            request(
                &router,
                &reviewer,
                "POST",
                &decision_path,
                decision.clone(),
                ""
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
    let history = body(
        request(
            &router,
            &requester,
            "GET",
            &format!("/api/product/approvals/{second_id}/history"),
            json!({}),
            "",
        )
        .await,
    )
    .await;
    assert_eq!(history["data"].as_array().unwrap().len(), 2);
    let task = database
        .fetch_optional(Sql::bind(
            "SELECT status, revision FROM tasks WHERE id = $1",
            [task_id.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.try_get::<String, _>("status").unwrap(), "done");
    assert_eq!(task.try_get::<i64, _>("revision").unwrap(), 3);
    let report_path = format!("/api/product/approvals/{second_id}/report");
    assert_eq!(
        request(&router, &reviewer, "GET", &report_path, json!({}), "")
            .await
            .status(),
        StatusCode::OK
    );
    let (mut outsider, _) = user(&router, "outsider@example.test").await;
    team(&router, &mut outsider).await;
    assert_eq!(
        request(&router, &outsider, "GET", &report_path, json!({}), "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &router,
            &requester,
            "POST",
            "/api/approval_requests/",
            json!({ "task_id_id": task_id, "report_run_id": run_id, "requested_by_id": actor, "reviewer_id_id": reviewer_id, "task_snapshot": {}, "state": "approved" }),
            ""
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    database.execute(Sql::bind("UPDATE \"_appstruct_report_runs\" SET expires_at = CURRENT_TIMESTAMP - INTERVAL '1 second' WHERE id = $1", [run_id.into()])).await.unwrap();
    assert_eq!(
        request(&router, &reviewer, "GET", &report_path, json!({}), "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
