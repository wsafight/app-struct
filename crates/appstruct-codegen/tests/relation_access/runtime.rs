use appstruct_generated_backend::{
    ApiError, AppExtensions, AuthState, RequestContext,
    api::task,
    database::{Database, DatabaseExecutor, Sql},
    entities::{project, task as task_entity},
    extensions::{ProjectPolicy, TaskHooks},
    router_with_auth,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use serde_json::{Value, json};
use sqlx::Row as _;
use std::sync::{Arc, Mutex};
use tower::ServiceExt as _;
use uuid::Uuid;

struct TargetPolicy;

#[async_trait]
impl ProjectPolicy for TargetPolicy {
    async fn can_read(
        &self,
        _ctx: &RequestContext,
        model: &project::Model,
    ) -> Result<bool, ApiError> {
        Ok(model.name != "denied")
    }
}

struct RelationHooks {
    denied: Arc<Mutex<Option<Uuid>>>,
}

impl RelationHooks {
    fn denied(&self) -> Uuid {
        self.denied.lock().unwrap().unwrap()
    }
}

#[async_trait]
impl TaskHooks for RelationHooks {
    async fn before_create(
        &self,
        _ctx: &RequestContext,
        input: &mut task::CreateInput,
    ) -> Result<(), ApiError> {
        if input.title == "hook-create" {
            input.project_id = Some(self.denied());
        }
        Ok(())
    }

    async fn before_update(
        &self,
        _ctx: &RequestContext,
        _before: &task_entity::Model,
        input: &mut task::UpdateInput,
    ) -> Result<(), ApiError> {
        if input.title.as_deref() == Some("hook-update") {
            input.project_id = Some(Some(self.denied()));
        }
        Ok(())
    }
}

#[derive(Default)]
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
    body: String,
    revision: Option<i64>,
    batch: Option<&str>,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(
            header::CONTENT_TYPE,
            if path.ends_with(".csv") {
                "text/csv"
            } else {
                "application/json"
            },
        )
        .header(header::COOKIE, &session.cookies)
        .header("x-csrf-token", &session.csrf)
        .header("x-appstruct-tenant", &session.tenant);
    if let Some(revision) = revision {
        request = request.header(header::IF_MATCH, format!("\"rev-{revision}\""));
    }
    if let Some(batch) = batch {
        request = request.header("idempotency-key", batch);
    }
    router
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn json_request(
    router: &Router,
    session: &Session,
    method: &str,
    path: &str,
    body: Value,
    revision: Option<i64>,
) -> Response {
    request(
        router,
        session,
        method,
        path,
        body.to_string(),
        revision,
        None,
    )
    .await
}

async fn csv_request(router: &Router, session: &Session, csv: String) -> Response {
    let batch = Uuid::now_v7().to_string();
    request(
        router,
        session,
        "POST",
        "/api/tasks/_import.csv",
        csv,
        None,
        Some(&batch),
    )
    .await
}

async fn json(response: Response) -> Value {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "invalid JSON response ({status}): {error}; body={:?}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

async fn register(router: &Router) -> (Session, Uuid) {
    let response = json_request(
        router,
        &Session::default(),
        "POST",
        "/api/auth/register",
        json!({"email":"relations@example.test","password":"AppStruct-relations-2026"}),
        None,
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
    let csrf = cookies
        .iter()
        .find_map(|value| value.strip_prefix("appstruct_csrf="))
        .unwrap()
        .to_owned();
    let body = json(response).await;
    let user = Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();
    (
        Session {
            cookies: cookies.join("; "),
            csrf,
            tenant: String::new(),
        },
        user,
    )
}

async fn create_project(router: &Router, session: &Session, owner: Uuid, name: &str) -> String {
    let response = json_request(
        router,
        session,
        "POST",
        "/api/projects/",
        json!({"name":name,"owner_id":owner}),
        None,
    )
    .await;
    let status = response.status();
    let body = json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn create_update_bulk_and_csv_validate_post_hook_relation_candidates() {
    let Ok(url) = std::env::var("APPSTRUCT_E2E_DATABASE_URL") else {
        return;
    };
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("relation_access_{}", Uuid::now_v7().simple());
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
    let denied = Arc::new(Mutex::new(None));
    let router = router_with_auth(
        database.clone(),
        AppExtensions::builder()
            .project_policy(TargetPolicy)
            .task_hooks(RelationHooks {
                denied: denied.clone(),
            })
            .build(),
        AuthState::from_env().unwrap(),
    )
    .unwrap();
    let (mut session, owner) = register(&router).await;
    let organization = json_request(
        &router,
        &session,
        "POST",
        "/api/tenant/organizations",
        json!({"name":"Relations Alpha"}),
        None,
    )
    .await;
    assert_eq!(organization.status(), StatusCode::CREATED);
    let alpha = json(organization).await["id"].as_str().unwrap().to_owned();
    session.tenant.clone_from(&alpha);
    let allowed = create_project(&router, &session, owner, "allowed").await;
    let denied_id = create_project(&router, &session, owner, "denied").await;
    *denied.lock().unwrap() = Some(Uuid::parse_str(&denied_id).unwrap());

    let organization = json_request(
        &router,
        &session,
        "POST",
        "/api/tenant/organizations",
        json!({"name":"Relations Beta"}),
        None,
    )
    .await;
    assert_eq!(organization.status(), StatusCode::CREATED);
    session.tenant = json(organization).await["id"].as_str().unwrap().to_owned();
    let foreign = create_project(&router, &session, owner, "foreign").await;
    session.tenant = alpha;

    let response = json_request(
        &router,
        &session,
        "POST",
        "/api/tasks/",
        json!({"title":"base","project_id":&allowed}),
        None,
    )
    .await;
    let status = response.status();
    let base = json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{base}");
    let task_id = base["id"].as_str().unwrap().to_owned();

    let response = json_request(
        &router,
        &session,
        "POST",
        "/api/tasks/",
        json!({"title":"nullable"}),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    for body in [
        json!({"title":"policy-create","project_id":&denied_id}),
        json!({"title":"hook-create","project_id":&allowed}),
        json!({"title":"tenant-create","project_id":&foreign}),
    ] {
        let response = json_request(&router, &session, "POST", "/api/tasks/", body, None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    let task_path = format!("/api/tasks/{task_id}");
    for body in [
        json!({"project_id":&denied_id}),
        json!({"title":"hook-update"}),
    ] {
        let response = json_request(&router, &session, "PATCH", &task_path, body, Some(1)).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    for patch in [
        json!({"project_id":&denied_id}),
        json!({"title":"hook-update"}),
    ] {
        let response = json_request(
            &router,
            &session,
            "PATCH",
            "/api/tasks/_bulk",
            json!({
                "ids":[&task_id],
                "expected_revisions":{(task_id.as_str()):1},
                "patch":patch
            }),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = json(response).await;
        assert!(result["succeeded"].as_array().unwrap().is_empty());
        assert_eq!(result["failed"][0]["code"], "not_found", "{result}");
    }

    for (title, relation) in [
        ("policy-create", denied_id.as_str()),
        ("hook-create", allowed.as_str()),
        ("tenant-create", foreign.as_str()),
    ] {
        let response = csv_request(
            &router,
            &session,
            format!("title,project_id\n{title},{relation}\n"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = json(response).await;
        assert!(result["succeeded"].as_array().unwrap().is_empty());
        assert_eq!(result["failed"][0]["code"], "not_found", "{result}");
    }

    let response = json_request(&router, &session, "GET", &task_path, Value::Null, None).await;
    let current = json(response).await;
    assert_eq!(current["title"], "base");
    assert_eq!(current["revision"], 1);
    let count = database
        .fetch_optional(Sql::new("SELECT count(*) AS count FROM tasks".to_owned()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.try_get::<i64, _>("count").unwrap(), 2);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
