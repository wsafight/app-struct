use super::*;
use appstruct_generated_backend::product::{notify, refund_credits, reserve_credits};
use uuid::Uuid;

const PASSWORD: &str = "Product-Capabilities-Password-2026";

async fn setup() -> Option<(Database, Database, String, Router)> {
    let url = std::env::var("APPSTRUCT_E2E_DATABASE_URL").ok()?;
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("capabilities_{}", Uuid::now_v7().simple());
    root.execute_unprepared(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let database = Database::connect(format!("{url}{separator}options=-csearch_path%3D{schema}")).await.unwrap();
    database.execute_unprepared(include_str!("../../../database/0001_initial.sql")).await.unwrap();
    let router = router_with_services(database.clone(), AppExtensions::builder().build(), AuthState::from_env().unwrap(), MailState::from_env(database.clone()).unwrap(), FileState::from_env(database.clone()).unwrap());
    Some((root, database, schema, router))
}

fn cookies(response: &Response, session: &mut Session) {
    let values = response.headers().get_all(header::SET_COOKIE).iter().map(|value| value.to_str().unwrap().split(';').next().unwrap().to_owned()).collect::<Vec<_>>();
    session.csrf = values.iter().find_map(|value| value.strip_prefix("appstruct_csrf=")).unwrap().to_owned();
    session.cookies = values.join("; ");
}

async fn user(router: &Router, name: &str) -> (Session, Uuid) {
    let mut session = Session { cookies: String::new(), csrf: String::new(), tenant: String::new() };
    let response = request(router, &session, "POST", "/api/auth/register", json!({ "email": format!("{name}@example.test"), "password": PASSWORD }), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    cookies(&response, &mut session);
    let value = json_body(response).await;
    (session, Uuid::parse_str(value["user"]["id"].as_str().unwrap()).unwrap())
}

async fn team(router: &Router, session: &mut Session) -> Uuid {
    let response = request(router, session, "POST", "/api/tenant/organizations", json!({ "name": "Capability team" }), "").await;
    assert_eq!(response.status(), StatusCode::CREATED);
    session.tenant = json_body(response).await["id"].as_str().unwrap().to_owned();
    Uuid::parse_str(&session.tenant).unwrap()
}

async fn bytes(router: &Router, session: &Session, method: &str, path: &str, content: &str) -> Response {
    router.clone().oneshot(Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/octet-stream").header(header::COOKIE, &session.cookies).header("x-csrf-token", &session.csrf).header("x-appstruct-tenant", &session.tenant).body(Body::from(content.to_owned())).unwrap()).await.unwrap()
}

async fn staged(router: &Router, session: &Session, content: &str) -> Value {
    let response = request(router, session, "POST", "/api/files/uploads", json!({ "name": "input.txt", "content_type": "text/plain", "size": content.len() }), "").await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let upload = json_body(response).await;
    assert_eq!(upload["direct"], false);
    let response = bytes(router, session, "PUT", upload["upload_url"].as_str().unwrap(), content).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let path = format!("/api/files/uploads/{}/complete", upload["id"].as_str().unwrap());
    let completed = request(router, session, "POST", &path, json!({}), "").await;
    assert_eq!(completed.status(), StatusCode::OK);
    let file = json_body(completed).await;
    assert_eq!(json_body(request(router, session, "POST", &path, json!({}), "").await).await, file);
    file
}

#[tokio::test]
async fn staged_uploads_attachments_replacement_and_expiry_are_scoped() {
    let Some((root, database, schema, router)) = setup().await else { return; };
    let (mut session, _) = user(&router, "files").await;
    let tenant = team(&router, &mut session).await;
    let (mut other, _) = user(&router, "other").await;
    team(&router, &mut other).await;
    let first = staged(&router, &session, "first input").await;
    let second = staged(&router, &session, "second input").await;
    let project = request(&router, &session, "POST", "/api/projects/", json!({ "name": "Attachment record", "status": "active" }), "").await;
    assert_eq!(project.status(), StatusCode::CREATED);
    let record = json_body(project).await["id"].as_str().unwrap().to_owned();
    let first_id = first["id"].as_str().unwrap();
    let second_id = second["id"].as_str().unwrap();
    let path = format!("/api/files/records/projects/{record}/{first_id}");
    assert_eq!(request(&router, &other, "PUT", &path, json!({}), "").await.status(), StatusCode::NOT_FOUND);
    let noncanonical = record.replace('-', "").to_uppercase();
    assert_eq!(request(&router, &session, "PUT", &format!("/api/files/records/projects/{noncanonical}/{first_id}"), json!({}), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &session, "PUT", &path, json!({}), "").await.status(), StatusCode::NO_CONTENT);
    let listing = json_body(request(&router, &session, "GET", &format!("/api/files/records/projects/{record}"), json!({}), "").await).await;
    assert_eq!(listing["data"].as_array().unwrap().len(), 1);
    assert_eq!(request(&router, &session, "DELETE", &format!("/api/files/{first_id}"), json!({}), "").await.status(), StatusCode::CONFLICT);
    assert_eq!(request(&router, &session, "GET", &format!("/api/files/{first_id}/download"), json!({}), "").await.status(), StatusCode::OK);
    for _ in 0..2 {
        assert_eq!(request(&router, &session, "PUT", &format!("{path}/replace"), json!({ "replacement_file_id": second_id }), "").await.status(), StatusCode::NO_CONTENT);
    }
    let listing = json_body(request(&router, &session, "GET", &format!("/api/files/records/projects/{record}"), json!({}), "").await).await;
    assert_eq!(listing["data"][0]["id"], second["id"]);
    assert_eq!(request(&router, &session, "DELETE", &format!("/api/files/{first_id}"), json!({}), "").await.status(), StatusCode::NO_CONTENT);
    database.execute(Sql::bind("DELETE FROM projects WHERE id = $1 AND tenant_id = $2", [Uuid::parse_str(&record).unwrap().into(), tenant.into()])).await.unwrap();
    assert_eq!(request(&router, &session, "DELETE", &format!("/api/files/{second_id}"), json!({}), "").await.status(), StatusCode::NO_CONTENT);
    for cancelled in [true, false] {
        let upload = json_body(request(&router, &session, "POST", "/api/files/uploads", json!({ "name": "expires.txt", "content_type": "text/plain", "size": 3 }), "").await).await;
        let id = upload["id"].as_str().unwrap();
        assert_eq!(request(&router, &other, "GET", &format!("/api/files/uploads/{id}"), json!({}), "").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(bytes(&router, &session, "PUT", upload["upload_url"].as_str().unwrap(), "bad size").await.status(), StatusCode::BAD_REQUEST);
        if cancelled {
            assert_eq!(request(&router, &session, "DELETE", &format!("/api/files/uploads/{id}"), json!({}), "").await.status(), StatusCode::NO_CONTENT);
        } else {
            database.execute(Sql::bind("UPDATE \"_appstruct_file_uploads\" SET expires_at = CURRENT_TIMESTAMP - INTERVAL '1 minute' WHERE id = $1", [Uuid::parse_str(id).unwrap().into()])).await.unwrap();
        }
        assert_eq!(request(&router, &session, "POST", &format!("/api/files/uploads/{id}/complete"), json!({}), "").await.status(), StatusCode::CONFLICT);
    }
    for _ in 0..20 {
        assert_eq!(request(&router, &session, "POST", "/api/files/uploads", json!({ "name": "pending.txt", "content_type": "text/plain", "size": 3 }), "").await.status(), StatusCode::CREATED);
    }
    assert_eq!(request(&router, &session, "POST", "/api/files/uploads", json!({ "name": "overflow.txt", "content_type": "text/plain", "size": 3 }), "").await.status(), StatusCode::CONFLICT);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

#[tokio::test]
async fn credits_serialize_reservations_refund_once_and_preserve_expiry() {
    let Some((root, database, schema, router)) = setup().await else { return; };
    let (mut session, actor) = user(&router, "credits").await;
    let tenant = team(&router, &mut session).await;
    let overview = json_body(request(&router, &session, "GET", "/api/product/credits", json!({}), "").await).await;
    assert_eq!(overview["monthly_remaining"], 20);
    let operation = Uuid::now_v7();
    let reserve = |database: Database| async move {
        let transaction = database.begin().await.unwrap();
        let created = reserve_credits(&transaction, tenant, actor, operation, 15).await.unwrap();
        transaction.commit().await.unwrap();
        created
    };
    let (first, second) = tokio::join!(reserve(database.clone()), reserve(database.clone()));
    assert_ne!(first, second);
    let transaction = database.begin().await.unwrap();
    assert!(reserve_credits(&transaction, tenant, actor, Uuid::now_v7(), 6).await.is_err());
    transaction.rollback().await.unwrap();
    for _ in 0..2 {
        let transaction = database.begin().await.unwrap();
        refund_credits(&transaction, tenant, operation).await.unwrap();
        transaction.commit().await.unwrap();
    }
    assert_eq!(json_body(request(&router, &session, "GET", "/api/product/credits", json!({}), "").await).await["monthly_remaining"], 20);
    let operation = Uuid::now_v7();
    let transaction = database.begin().await.unwrap();
    reserve_credits(&transaction, tenant, actor, operation, 5).await.unwrap();
    transaction.commit().await.unwrap();
    database.execute(Sql::bind("UPDATE \"_appstruct_product_credit_reservations\" SET expires_at = CURRENT_TIMESTAMP - INTERVAL '1 day' WHERE id = $1", [operation.into()])).await.unwrap();
    database.execute_unprepared("UPDATE \"_appstruct_product_credit_ledger\" SET expires_at = CURRENT_TIMESTAMP - INTERVAL '1 day' WHERE expires_at IS NOT NULL").await.unwrap();
    let transaction = database.begin().await.unwrap();
    refund_credits(&transaction, tenant, operation).await.unwrap();
    transaction.commit().await.unwrap();
    let balance = database.fetch_optional(Sql::bind("SELECT COALESCE(SUM(amount), 0)::bigint AS total FROM \"_appstruct_product_credit_ledger\" WHERE tenant_id = $1 AND expires_at > CURRENT_TIMESTAMP", [tenant.into()])).await.unwrap().unwrap().try_get::<i64, _>("total").unwrap();
    assert_eq!(balance, 0);
    notify(&database, actor, Some(tenant), "test", "Ready", "Result ready", "/results").await.unwrap();
    notify(&database, actor, Some(tenant), "test", "Ready", "Result ready", "/results").await.unwrap();
    let inbox = json_body(request(&router, &session, "GET", "/api/product/notifications", json!({}), "").await).await;
    assert_eq!(inbox["unread"], 1);
    assert_eq!(request(&router, &session, "POST", "/api/product/notifications/read", json!({}), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(json_body(request(&router, &session, "GET", "/api/product/notifications", json!({}), "").await).await["unread"], 0);
    assert_eq!(request(&router, &session, "PATCH", "/api/auth/settings", json!({ "display_name": "Credit user", "locale": "zh-CN", "timezone": "Asia/Shanghai", "notifications": false }), "").await.status(), StatusCode::NO_CONTENT);
    notify(&database, actor, Some(tenant), "muted", "Muted", "No future notifications", "/results").await.unwrap();
    assert_eq!(json_body(request(&router, &session, "GET", "/api/product/notifications", json!({}), "").await).await["total"], 1);
    let report = request(&router, &session, "POST", "/api/reports/templates/team-summary/runs", json!({ "data": { "title": "Preferences", "summary": "Saved profile defaults" } }), "profile-defaults").await;
    assert_eq!(report.status(), StatusCode::ACCEPTED);
    let report = json_body(report).await;
    assert_eq!(report["locale"], "zh-CN");
    assert_eq!(report["timezone"], "Asia/Shanghai");
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

#[tokio::test]
async fn payment_events_are_verified_idempotent_and_order_independent() {
    let Some((root, database, schema, router)) = setup().await else { return; };
    let (mut session, actor) = user(&router, "payments").await;
    let tenant = team(&router, &mut session).await;
    database.execute(Sql::bind("INSERT INTO \"_appstruct_billing_customers\" (id, organization_id, provider_customer_id, created_at) VALUES ($1, $2, 'cus_capabilities', CURRENT_TIMESTAMP)", [Uuid::now_v7().into(), tenant.into()])).await.unwrap();
    database.execute(Sql::bind("INSERT INTO \"_appstruct_product_credit_purchases\" (tenant_id, session_id, credits, fulfilled, refunded_credits, created_at) VALUES ($1, 'cs_capabilities', 100, false, 0, CURRENT_TIMESTAMP)", [tenant.into()])).await.unwrap();
    let event = |id: &str, kind: &str, object: Value| json!({ "id": id, "type": kind, "data": { "object": object } });
    let charge = |refunded: i64| json!({ "id": "ch_capabilities", "customer": "cus_capabilities", "paid": true, "payment_intent": "pi_capabilities", "currency": "usd", "amount_captured": 1000, "amount_refunded": refunded, "created": chrono::Utc::now().timestamp() });
    let apply = |event: Value| {
        let database = database.clone();
        async move { appstruct_generated_backend::product::apply_billing_event(&database, &event).await.unwrap(); }
    };
    apply(event("evt_refund_first", "charge.refunded", charge(500))).await;
    apply(event("evt_stale_charge", "charge.succeeded", charge(0))).await;
    let checkout = json!({ "id": "cs_capabilities", "mode": "payment", "payment_status": "paid", "payment_intent": "pi_capabilities", "metadata": { "appstruct_credit_pack": "v1" } });
    let completed = event("evt_complete", "checkout.session.completed", checkout.clone());
    tokio::join!(apply(completed.clone()), apply(completed));
    let credits = json_body(request(&router, &session, "GET", "/api/product/credits", json!({}), "").await).await;
    assert_eq!(credits["paid_balance"], 50);
    assert_eq!(credits["monthly_remaining"], 20);
    let transaction = database.begin().await.unwrap();
    reserve_credits(&transaction, tenant, actor, Uuid::now_v7(), 70).await.unwrap();
    transaction.commit().await.unwrap();
    apply(event("evt_full_refund", "charge.refunded", charge(1000))).await;
    apply(event("evt_stale_checkout", "checkout.session.async_payment_succeeded", checkout)).await;
    let credits = json_body(request(&router, &session, "GET", "/api/product/credits", json!({}), "").await).await;
    assert_eq!(credits["paid_balance"], -50);
    let transaction = database.begin().await.unwrap();
    assert!(reserve_credits(&transaction, tenant, actor, Uuid::now_v7(), 1).await.is_err());
    transaction.rollback().await.unwrap();
    let payment = database.fetch_optional(Sql::new("SELECT captured, refunded FROM \"_appstruct_product_payments\" WHERE charge_id = 'ch_capabilities'")).await.unwrap().unwrap();
    assert_eq!(payment.try_get::<i64, _>("captured").unwrap(), 1000);
    assert_eq!(payment.try_get::<i64, _>("refunded").unwrap(), 1000);
    apply(event("evt_unpaid", "checkout.session.completed", json!({ "id": "cs_unknown", "mode": "payment", "payment_status": "unpaid", "metadata": { "appstruct_credit_pack": "v1" } }))).await;
    if let Ok(secret) = std::env::var("APPSTRUCT_STRIPE_WEBHOOK_SECRET") {
        let payload = event("evt_http_signed", "charge.succeeded", charge(0)).to_string();
        let timestamp = chrono::Utc::now().timestamp();
        let signature = ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes()), format!("{timestamp}.{payload}").as_bytes());
        let signature = signature.as_ref().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        for (signature, expected) in [(format!("t={timestamp},v1={signature}"), StatusCode::NO_CONTENT), (format!("t={timestamp},v1=00"), StatusCode::FORBIDDEN)] {
            let response = router.clone().oneshot(Request::builder().method("POST").uri("/api/billing/stripe/webhook").header("stripe-signature", signature).body(Body::from(payload.clone())).unwrap()).await.unwrap();
            assert_eq!(response.status(), expected);
        }
    }
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

#[tokio::test]
async fn team_transfer_removal_and_invitation_capacity_are_serialized() {
    let Some((root, database, schema, router)) = setup().await else { return; };
    let (mut owner, owner_id) = user(&router, "owner").await;
    let tenant = team(&router, &mut owner).await;
    let (mut member, member_id) = user(&router, "member").await;
    member.tenant.clone_from(&owner.tenant);
    assert_eq!(request(&router, &member, "GET", "/api/product/team", json!({}), "").await.status(), StatusCode::FORBIDDEN);
    database.execute(Sql::bind("INSERT INTO \"_appstruct_tenant_memberships\" (organization_id, user_id, role, created_at) VALUES ($1, $2, 'member', CURRENT_TIMESTAMP)", [tenant.into(), member_id.into()])).await.unwrap();
    assert_eq!(request(&router, &member, "PATCH", "/api/product/team", json!({ "name": "Not allowed" }), "").await.status(), StatusCode::FORBIDDEN);
    assert_eq!(request(&router, &owner, "DELETE", &format!("/api/product/team/members/{owner_id}"), json!({}), "").await.status(), StatusCode::CONFLICT);
    assert_eq!(request(&router, &owner, "POST", "/api/product/team/transfer", json!({ "member_id": member_id }), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &owner, "PATCH", "/api/product/team", json!({ "name": "Stale owner" }), "").await.status(), StatusCode::FORBIDDEN);
    assert_eq!(request(&router, &member, "PATCH", "/api/product/team", json!({ "name": "Transferred team" }), "").await.status(), StatusCode::NO_CONTENT);
    if std::env::var("APPSTRUCT_TEAM_SEAT_LIMIT").as_deref() == Ok("2") {
        assert_eq!(request(&router, &member, "POST", "/api/tenant/invitations", json!({ "email": "next@example.test", "role": "member" }), "").await.status(), StatusCode::PAYMENT_REQUIRED);
    }
    assert_eq!(request(&router, &member, "DELETE", &format!("/api/product/team/members/{owner_id}"), json!({}), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &owner, "GET", "/api/product/team", json!({}), "").await.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(request(&router, &member, "GET", "/api/product/team", json!({}), "").await).await["name"], "Transferred team");
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

#[tokio::test]
async fn consented_visits_and_fully_observed_retention_have_explicit_scopes() {
    let Some((root, database, schema, router)) = setup().await else { return; };
    let (admin, actor) = user(&router, "analytics").await;
    assert_eq!(request(&router, &admin, "GET", "/api/product/insights", json!({}), "").await.status(), StatusCode::FORBIDDEN);
    database.execute(Sql::bind("UPDATE \"_appstruct_auth_accounts\" SET roles = '[\"admin\"]'::jsonb WHERE user_id = $1", [actor.into()])).await.unwrap();
    let visitor = Uuid::now_v7();
    assert_eq!(request(&router, &admin, "POST", "/api/product/visits", json!({ "visitor_id": visitor, "page": "/", "consent": false }), "").await.status(), StatusCode::BAD_REQUEST);
    for _ in 0..2 { assert_eq!(request(&router, &admin, "POST", "/api/product/visits", json!({ "visitor_id": visitor, "page": "/", "consent": true }), "").await.status(), StatusCode::NO_CONTENT); }
    appstruct_generated_backend::product::record_event(&database, Some(actor), Some(visitor), None, "cohort", "activation").await.unwrap();
    database.execute_unprepared("UPDATE \"_appstruct_product_events\" SET occurred_at = CURRENT_TIMESTAMP - INTERVAL '10 days' WHERE event_key = 'cohort'").await.unwrap();
    appstruct_generated_backend::product::record_event(&database, Some(actor), None, None, "returned", "activation").await.unwrap();
    database.execute_unprepared("UPDATE \"_appstruct_product_events\" SET occurred_at = CURRENT_TIMESTAMP - INTERVAL '2 days 12 hours' WHERE event_key = 'returned'").await.unwrap();
    let insight = request(&router, &admin, "GET", "/api/product/insights", json!({}), "").await;
    assert_eq!(insight.status(), StatusCode::OK);
    let insight = json_body(insight).await;
    assert_eq!(insight["visitors"], 1);
    assert_eq!(insight["signups"], 1);
    assert_eq!(insight["retention_d7"]["eligible"], 1);
    assert_eq!(insight["retention_d7"]["returned"], 1);
    assert!(insight["definitions"]["funnel"].as_str().unwrap().contains("not an attributed"));
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

#[tokio::test]
async fn concurrent_invitation_acceptance_respects_the_last_seat() {
    if std::env::var("APPSTRUCT_TEAM_SEAT_LIMIT").as_deref() != Ok("2") { return; }
    let Some((root, database, schema, router)) = setup().await else { return; };
    let (mut owner, _) = user(&router, "seat-owner").await;
    let tenant = team(&router, &mut owner).await;
    let (first, _) = user(&router, "seat-first").await;
    let (second, _) = user(&router, "seat-second").await;
    let mut tokens = Vec::new();
    for email in ["seat-first@example.test", "seat-second@example.test"] {
        assert_eq!(request(&router, &owner, "POST", "/api/tenant/invitations", json!({ "email": email, "role": "member" }), "").await.status(), StatusCode::CREATED);
        let body = database.fetch_optional(Sql::bind("SELECT body FROM \"_appstruct_auth_mail_capture\" WHERE recipient = $1 ORDER BY created_at DESC LIMIT 1", [email.into()])).await.unwrap().unwrap().try_get::<String, _>("body").unwrap();
        tokens.push(body.split("token=").nth(1).unwrap().to_owned());
    }
    let first_path = format!("/api/tenant/invitations/{}/accept", tokens[0]);
    let second_path = format!("/api/tenant/invitations/{}/accept", tokens[1]);
    let (first, second) = tokio::join!(request(&router, &first, "POST", &first_path, json!({}), ""), request(&router, &second, "POST", &second_path, json!({}), ""));
    let statuses = [first.status(), second.status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::PAYMENT_REQUIRED));
    let count = database.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1", [tenant.into()])).await.unwrap().unwrap().try_get::<i64, _>("total").unwrap();
    assert_eq!(count, 2);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}

fn authenticator(secret: &str) -> String {
    let bytes = data_encoding::BASE32_NOPAD.decode(secret.as_bytes()).unwrap();
    let counter = (chrono::Utc::now().timestamp() as u64 / 30).to_be_bytes();
    let signature = ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &bytes), &counter);
    let bytes = signature.as_ref();
    let offset = usize::from(bytes[19] & 15);
    let value = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    format!("{:06}", value % 1_000_000)
}

#[tokio::test]
async fn mfa_recovery_password_email_and_session_changes_fail_closed() {
    let Some((root, database, schema, router)) = setup().await else { return; };
    if std::env::var("APPSTRUCT_AUTH_MFA_KEY").is_err() { root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap(); return; }
    let (mut session, _) = user(&router, "security").await;
    let original = session.clone();
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/mfa/setup", json!({ "password": "incorrect" }), "").await.status(), StatusCode::UNAUTHORIZED);
    let response = request(&router, &session, "POST", "/api/auth/settings/mfa/setup", json!({ "password": PASSWORD }), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let setup = json_body(response).await;
    let code = authenticator(setup["secret"].as_str().unwrap());
    let response = request(&router, &session, "POST", "/api/auth/settings/mfa/enable", json!({ "code": code }), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    cookies(&response, &mut session);
    let recovery = json_body(response).await["recovery_codes"].as_array().unwrap().clone();
    assert_eq!(recovery.len(), 8);
    assert_eq!(request(&router, &original, "GET", "/api/auth/settings", json!({}), "").await.status(), StatusCode::UNAUTHORIZED);
    let mut anonymous = Session { cookies: String::new(), csrf: String::new(), tenant: String::new() };
    for input in [json!({ "email": "security@example.test", "password": PASSWORD }), json!({ "email": "security@example.test", "password": PASSWORD, "code": code })] {
        assert_eq!(request(&router, &anonymous, "POST", "/api/auth/login", input, "").await.status(), StatusCode::UNAUTHORIZED);
    }
    let login = json!({ "email": "security@example.test", "password": PASSWORD, "code": recovery[0] });
    let response = request(&router, &anonymous, "POST", "/api/auth/login", login.clone(), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    cookies(&response, &mut anonymous);
    assert_eq!(request(&router, &anonymous, "POST", "/api/auth/login", login, "").await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/mfa/disable", json!({ "password": PASSWORD, "code": recovery[1] }), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &anonymous, "GET", "/api/auth/settings", json!({}), "").await.status(), StatusCode::UNAUTHORIZED);
    let response = request(&router, &session, "POST", "/api/auth/login", json!({ "email": "security@example.test", "password": PASSWORD }), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    cookies(&response, &mut session);
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/email", json!({ "password": PASSWORD, "email": "changed@example.test" }), "").await.status(), StatusCode::NO_CONTENT);
    let body = database.fetch_optional(Sql::new("SELECT body FROM \"_appstruct_auth_mail_capture\" WHERE recipient = 'changed@example.test' ORDER BY created_at DESC LIMIT 1")).await.unwrap().unwrap().try_get::<String, _>("body").unwrap();
    let token = body.split("email_token=").nth(1).unwrap().split_whitespace().next().unwrap();
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/email/confirm", json!({ "token": token }), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &session, "POST", "/api/auth/login", json!({ "email": "security@example.test", "password": PASSWORD }), "").await.status(), StatusCode::UNAUTHORIZED);
    let response = request(&router, &session, "POST", "/api/auth/login", json!({ "email": "changed@example.test", "password": PASSWORD }), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    cookies(&response, &mut session);
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/email/confirm", json!({ "token": token }), "").await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/password", json!({ "password": PASSWORD, "new_password": "A-New-Secure-Password-2026" }), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &session, "GET", "/api/auth/settings", json!({}), "").await.status(), StatusCode::UNAUTHORIZED);
    let response = request(&router, &session, "POST", "/api/auth/login", json!({ "email": "changed@example.test", "password": "A-New-Secure-Password-2026" }), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    cookies(&response, &mut session);
    assert_eq!(request(&router, &session, "POST", "/api/auth/settings/sessions/revoke", json!({ "password": "A-New-Secure-Password-2026" }), "").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(request(&router, &session, "GET", "/api/auth/settings", json!({}), "").await.status(), StatusCode::UNAUTHORIZED);
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE")).await.unwrap();
}
