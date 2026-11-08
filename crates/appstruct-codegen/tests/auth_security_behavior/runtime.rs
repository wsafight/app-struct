use appstruct_generated_backend::{
    AppExtensions, AuthState,
    database::{Database, DatabaseExecutor, Sql},
    router_with_auth,
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

const PASSWORD: &str = "Auth-Security-Password-2026";

#[derive(Clone, Default)]
struct Session {
    cookies: String,
    csrf: String,
    tenant: String,
}

async fn setup() -> Option<(Database, Database, String, Router)> {
    let url = std::env::var("APPSTRUCT_E2E_DATABASE_URL").ok()?;
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("auth_security_{}", Uuid::now_v7().simple());
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
    let router = router_with_auth(
        database.clone(),
        AppExtensions::builder().build(),
        AuthState::from_env().unwrap(),
    )
    .unwrap();
    Some((root, database, schema, router))
}

async fn request(
    router: &Router,
    session: &Session,
    method: &str,
    path: &str,
    body: Value,
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
    router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap()
}

async fn user(router: &Router, email: &str) -> (Session, Uuid) {
    let mut session = Session::default();
    let response = request(
        router,
        &session,
        "POST",
        "/api/auth/register",
        json!({ "email": email, "password": PASSWORD }),
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
    let body = json_body(response).await;
    let user_id = Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();
    (session, user_id)
}

async fn team(router: &Router, session: &mut Session) {
    let response = request(
        router,
        session,
        "POST",
        "/api/tenant/organizations",
        json!({ "name": "Auth security" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    session.tenant = json_body(response).await["id"].as_str().unwrap().to_owned();
}

#[tokio::test]
async fn provider_keys_are_composite_and_concurrent_unlink_preserves_a_login_method() {
    let Some((root, database, schema, router)) = setup().await else {
        return;
    };
    let (session, actor) = user(&router, "oauth-unlink@example.test").await;
    database
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_accounts\" SET password_hash = '' WHERE user_id = $1",
            [actor.into()],
        ))
        .await
        .unwrap();
    for provider in ["oidc", "google"] {
        database
            .execute(Sql::bind(
                "INSERT INTO \"_appstruct_auth_oauth_accounts\" (provider, subject, user_id, created_at) VALUES ($1, 'shared-subject', $2, CURRENT_TIMESTAMP)",
                [provider.into(), actor.into()],
            ))
            .await
            .unwrap();
    }
    assert!(
        database
            .execute(Sql::bind(
                "INSERT INTO \"_appstruct_auth_oauth_accounts\" (provider, subject, user_id, created_at) VALUES ('oidc', 'shared-subject', $1, CURRENT_TIMESTAMP)",
                [actor.into()],
            ))
            .await
            .is_err()
    );

    let (first, second) = tokio::join!(
        request(
            &router,
            &session,
            "DELETE",
            "/api/auth/accounts/oidc",
            json!({})
        ),
        request(
            &router,
            &session,
            "DELETE",
            "/api/auth/accounts/google",
            json!({})
        )
    );
    let statuses = [first.status(), second.status()];
    assert!(statuses.contains(&StatusCode::NO_CONTENT));
    assert!(statuses.contains(&StatusCode::CONFLICT));
    let remaining = database
        .fetch_optional(Sql::bind(
            "SELECT COUNT(*) AS total FROM \"_appstruct_auth_oauth_accounts\" WHERE user_id = $1",
            [actor.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(remaining.try_get::<i64, _>("total").unwrap(), 1);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}

#[tokio::test]
async fn invitations_require_verified_email_before_token_delivery_or_acceptance() {
    let Some((root, database, schema, router)) = setup().await else {
        return;
    };
    let (mut owner, _) = user(&router, "invitation-owner@example.test").await;
    team(&router, &mut owner).await;
    let (member, member_id) = user(&router, "invitation-member@example.test").await;
    let email = "invitation-member@example.test";
    let response = request(
        &router,
        &owner,
        "POST",
        "/api/tenant/invitations",
        json!({ "email": email, "role": "member" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let notifications = database
        .fetch_optional(Sql::bind(
            "SELECT COUNT(*) AS total FROM \"_appstruct_product_notifications\" WHERE user_id = $1 AND event_key LIKE 'invitation:%'",
            [member_id.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(notifications.try_get::<i64, _>("total").unwrap(), 0);
    let body = database
        .fetch_optional(Sql::bind(
            "SELECT body FROM \"_appstruct_auth_mail_capture\" WHERE recipient = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
            [email.into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String, _>("body")
        .unwrap();
    let first_token = body
        .split("token=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    let response = request(
        &router,
        &member,
        "POST",
        &format!("/api/tenant/invitations/{first_token}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let memberships = database
        .fetch_optional(Sql::bind(
            "SELECT COUNT(*) AS total FROM \"_appstruct_tenant_memberships\" WHERE user_id = $1",
            [member_id.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(memberships.try_get::<i64, _>("total").unwrap(), 0);

    database
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_accounts\" SET email_verified_at = CURRENT_TIMESTAMP WHERE user_id = $1",
            [member_id.into()],
        ))
        .await
        .unwrap();
    let response = request(
        &router,
        &owner,
        "POST",
        "/api/tenant/invitations",
        json!({ "email": email, "role": "member" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let invitation = json_body(response).await;
    let event_key = format!(
        "{member_id}:invitation:{}",
        invitation["id"].as_str().unwrap()
    );
    let notification = database
        .fetch_optional(Sql::bind(
            "SELECT target FROM \"_appstruct_product_notifications\" WHERE user_id = $1 AND event_key = $2",
            [member_id.into(), event_key.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    let target = notification.try_get::<String, _>("target").unwrap();
    let body = database
        .fetch_optional(Sql::bind(
            "SELECT body FROM \"_appstruct_auth_mail_capture\" WHERE recipient = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
            [email.into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String, _>("body")
        .unwrap();
    let token = body
        .split("token=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert_eq!(target, format!("/accept-invitation?token={token}"));
    let response = request(
        &router,
        &member,
        "POST",
        &format!("/api/tenant/invitations/{token}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
