use appstruct_generated_backend::{
    AppExtensions, AuthState, FileError, FileProvider, FileState, MailState, StoredObject,
    database::{Database, DatabaseExecutor, Sql},
    router_with_services,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use futures_util::{StreamExt as _, stream::BoxStream};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tower::ServiceExt as _;
use uuid::Uuid;

const PASSWORD: &str = "Lifecycle-Semantic-2026";

#[derive(Default)]
struct MemoryStorage {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
}

#[async_trait::async_trait]
impl FileProvider for MemoryStorage {
    fn name(&self) -> &'static str {
        "memory"
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), FileError> {
        self.objects.lock().unwrap().insert(key.to_owned(), bytes.to_vec());
        Ok(())
    }
    fn list(&self) -> BoxStream<'static, Result<StoredObject, FileError>> {
        let items: Vec<_> = self
            .objects
            .lock()
            .unwrap()
            .iter()
            .map(|(key, bytes)| {
                Ok(StoredObject {
                    key: key.clone(),
                    size: bytes.len() as u64,
                    last_modified: chrono::Utc::now(),
                })
            })
            .collect();
        futures_util::stream::iter(items).boxed()
    }
    async fn get(&self, key: &str) -> Result<Vec<u8>, FileError> {
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| FileError::Storage("missing".to_owned()))
    }
    async fn delete(&self, key: &str) -> Result<(), FileError> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Session {
    cookies: String,
    csrf: String,
    tenant: String,
}

async fn setup() -> Option<(Database, Database, String, Router)> {
    let url = std::env::var("APPSTRUCT_E2E_DATABASE_URL").ok()?;
    let root = Database::connect(&url).await.unwrap();
    let schema = format!("semantic_lifecycle_{}", Uuid::now_v7().simple());
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
    let storage = Arc::new(MemoryStorage::default());
    let files = FileState::with_provider(
        database.clone(),
        storage,
        vec!["text/plain".to_owned(), "application/pdf".to_owned()],
    );
    let router = router_with_services(
        database.clone(),
        AppExtensions::builder().build(),
        AuthState::from_env().unwrap(),
        MailState::from_env(database.clone()).unwrap(),
        files,
    );
    Some((root, database, schema, router))
}

async fn send(
    router: &Router,
    session: &Session,
    method: &str,
    path: &str,
    content_type: &str,
    if_match: Option<&str>,
    body: Vec<u8>,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::COOKIE, &session.cookies)
        .header("x-csrf-token", &session.csrf);
    if !session.tenant.is_empty() {
        builder = builder.header("x-appstruct-tenant", &session.tenant);
    }
    if let Some(etag) = if_match {
        builder = builder.header(header::IF_MATCH, etag);
    }
    if path.ends_with("/_import.csv") {
        builder = builder.header("idempotency-key", Uuid::now_v7().to_string());
    }
    router
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn json_req(
    router: &Router,
    session: &Session,
    method: &str,
    path: &str,
    if_match: Option<&str>,
    body: Value,
) -> Response {
    send(
        router,
        session,
        method,
        path,
        "application/json",
        if_match,
        body.to_string().into_bytes(),
    )
    .await
}

async fn body_json(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
}

async fn register_admin_operator(
    router: &Router,
    database: &Database,
    email: &str,
) -> (Session, Uuid) {
    let mut session = Session::default();
    let res = json_req(
        router,
        &session,
        "POST",
        "/api/auth/register",
        None,
        json!({ "email": email, "password": PASSWORD }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let cookies: Vec<_> = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect();
    session.csrf = cookies
        .iter()
        .find_map(|v| v.strip_prefix("appstruct_csrf="))
        .unwrap()
        .to_owned();
    session.cookies = cookies.join("; ");
    let user_id = Uuid::parse_str(body_json(res).await["user"]["id"].as_str().unwrap()).unwrap();
    database
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_accounts\" SET roles = '[\"operator\", \"admin\"]'::jsonb WHERE user_id = $1",
            [user_id.into()],
        ))
        .await
        .unwrap();
    let org = json_req(
        router,
        &session,
        "POST",
        "/api/tenant/organizations",
        None,
        json!({ "name": "Ops Workspace" }),
    )
    .await;
    assert_eq!(org.status(), StatusCode::CREATED);
    session.tenant = body_json(org).await["id"].as_str().unwrap().to_owned();
    (session, user_id)
}

async fn upload_text(router: &Router, session: &Session, name: &str, content: &str) -> Uuid {
    let res = send(
        router,
        session,
        "POST",
        &format!("/api/files?name={name}"),
        "text/plain",
        None,
        content.as_bytes().to_vec(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    Uuid::parse_str(body_json(res).await["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn file_semantic_crud_multi_field_bulk_and_csv_preserve_reference_integrity() {
    let Some((root, database, schema, router)) = setup().await else {
        return;
    };
    let (session, _) = register_admin_operator(&router, &database, "files@ops.test").await;
    let (_, _) = register_admin_operator(&router, &database, "other-tenant@ops.test").await;

    let file_a = upload_text(&router, &session, "spec-a.txt", "spec A").await;
    let file_b = upload_text(&router, &session, "spec-b.txt", "spec B").await;
    let file_c = upload_text(&router, &session, "spec-c.txt", "spec C").await;
    let file_csv = upload_text(&router, &session, "spec-csv.txt", "spec CSV").await;

    // Missing or cross-tenant file UUID fails validation with field path and rolls back insert:
    let bad = json_req(
        &router,
        &session,
        "POST",
        "/api/products/",
        None,
        json!({ "sku": "BAD-FILE", "name": "Bad", "unit": "each", "datasheet": Uuid::now_v7() }),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(bad).await["error"]["fields"][0]["field"], "datasheet");

    // Two file fields on the SAME record referencing the SAME file keep independent owner leases:
    let created = json_req(
        &router,
        &session,
        "POST",
        "/api/products/",
        None,
        json!({
            "sku": "PROD-1",
            "name": "Dual file product",
            "unit": "each",
            "datasheet": file_a,
            "manual": file_a
        }),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let product = body_json(created).await;
    let product_id = product["id"].as_str().unwrap().to_owned();

    // Deleting file_a while referenced returns 409 Conflict:
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_a}"), None, json!({}))
            .await
            .status(),
        StatusCode::CONFLICT
    );

    // Clearing ONLY `datasheet` (null) while `manual` still points to file_a must keep file_a protected:
    let updated = json_req(
        &router,
        &session,
        "PATCH",
        &format!("/api/products/{product_id}"),
        Some("\"rev-1\""),
        json!({ "datasheet": null }),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_a}"), None, json!({}))
            .await
            .status(),
        StatusCode::CONFLICT
    );

    // Switching `manual` from file_a to file_b releases file_a and retains file_b:
    let updated = json_req(
        &router,
        &session,
        "PATCH",
        &format!("/api/products/{product_id}"),
        Some("\"rev-2\""),
        json!({ "manual": file_b }),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_a}"), None, json!({}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_b}"), None, json!({}))
            .await
            .status(),
        StatusCode::CONFLICT
    );

    // Bulk update switching `manual` to file_c releases file_b and retains file_c:
    let bulk_up = json_req(
        &router,
        &session,
        "PATCH",
        "/api/products/_bulk",
        None,
        json!({
            "ids": [product_id],
            "expected_revisions": { (&product_id): 3 },
            "patch": { "manual": file_c }
        }),
    )
    .await;
    assert_eq!(bulk_up.status(), StatusCode::OK);
    assert_eq!(body_json(bulk_up).await["succeeded"], json!([product_id]));
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_b}"), None, json!({}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_c}"), None, json!({}))
            .await
            .status(),
        StatusCode::CONFLICT
    );

    // Bulk delete releases file_c:
    let bulk_del = json_req(
        &router,
        &session,
        "DELETE",
        "/api/products/_bulk",
        None,
        json!({
            "ids": [product_id],
            "expected_revisions": { (&product_id): 4 }
        }),
    )
    .await;
    assert_eq!(bulk_del.status(), StatusCode::OK);
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_c}"), None, json!({}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );

    // CSV import retains valid file references and isolates rows with invalid file UUIDs:
    let csv_payload = format!(
        "sku,name,unit,datasheet\nCSV-OK,Valid CSV,each,{file_csv}\nCSV-BAD,Invalid CSV,each,{}\n",
        Uuid::now_v7()
    );
    let csv_res = send(
        &router,
        &session,
        "POST",
        "/api/products/_import.csv",
        "text/csv",
        None,
        csv_payload.into_bytes(),
    )
    .await;
    assert_eq!(csv_res.status(), StatusCode::OK);
    let csv_body = body_json(csv_res).await;
    assert_eq!(csv_body["succeeded"], json!(["0"]));
    assert_eq!(csv_body["failed"][0]["id"], "1");
    assert_eq!(
        json_req(&router, &session, "DELETE", &format!("/api/files/{file_csv}"), None, json!({}))
            .await
            .status(),
        StatusCode::CONFLICT
    );

    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn aggregate_create_rollback_cascade_file_release_and_bounded_list_counts_hold() {
    let Some((root, database, schema, router)) = setup().await else {
        return;
    };
    let (session, user_id) = register_admin_operator(&router, &database, "agg@ops.test").await;

    // Seed 5 products and verify exact vs bounded list count modes:
    let mut first_product = String::new();
    for idx in 1..=5 {
        let res = json_req(
            &router,
            &session,
            "POST",
            "/api/products/",
            None,
            json!({ "sku": format!("SKU-{idx}"), "name": format!("Item {idx}"), "unit": "each" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        if idx == 1 {
            first_product = body_json(res).await["id"].as_str().unwrap().to_owned();
        }
    }

    let exact_default = json_req(&router, &session, "GET", "/api/products/?page=1&page_size=2", None, json!({})).await;
    assert_eq!(exact_default.status(), StatusCode::OK);
    assert_eq!(
        body_json(exact_default).await["meta"],
        json!({ "page": 1, "page_size": 2, "total": 5 })
    );

    let bounded_under = json_req(
        &router,
        &session,
        "GET",
        "/api/products/?page=1&page_size=2&count=bounded",
        None,
        json!({}),
    )
    .await;
    assert_eq!(bounded_under.status(), StatusCode::OK);
    assert_eq!(
        body_json(bounded_under).await["meta"],
        json!({ "page": 1, "page_size": 2, "total": 5, "total_exact": true })
    );

    // Cap via APPSTRUCT_LIST_COUNT_LIMIT=3 and verify total_exact=false, plus count=exact override:
    unsafe { std::env::set_var("APPSTRUCT_LIST_COUNT_LIMIT", "3") };
    let capped = json_req(&router, &session, "GET", "/api/products/?page=1&page_size=2", None, json!({})).await;
    let exact_override = json_req(
        &router,
        &session,
        "GET",
        "/api/products/?page=1&page_size=2&count=exact",
        None,
        json!({}),
    )
    .await;
    unsafe { std::env::remove_var("APPSTRUCT_LIST_COUNT_LIMIT") };
    assert_eq!(
        body_json(capped).await["meta"],
        json!({ "page": 1, "page_size": 2, "total": 3, "total_exact": false })
    );
    assert_eq!(
        body_json(exact_override).await["meta"],
        json!({ "page": 1, "page_size": 2, "total": 5 })
    );

    for bad_query in ["/api/products/?count=approx", "/api/products/?limit=2&count=bounded"] {
        assert_eq!(
            json_req(&router, &session, "GET", bad_query, None, json!({}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }

    // Atomic POST /api/orders/_aggregates/lines rolls back parent + file retention when a child fails:
    let order_file = upload_text(&router, &session, "order.txt", "order header").await;
    let line_file = upload_text(&router, &session, "line.txt", "line spec").await;
    let failed_agg = json_req(
        &router,
        &session,
        "POST",
        "/api/orders/_aggregates/lines",
        None,
        json!({
            "parent": {
                "number": "ORD-ROLLBACK",
                "owner_id": user_id,
                "attachment": order_file
            },
            "creates": [{
                "key": "bad",
                "input": {
                    "product_id": first_product,
                    "quantity": 0,
                    "unit_price": "10.00",
                    "currency": "CNY"
                }
            }]
        }),
    )
    .await;
    assert_eq!(failed_agg.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(failed_agg).await["error"]["fields"][0]["field"],
        "creates.bad.quantity"
    );

    // Because the parent insert + retain_field_file rolled back, order_file has no business lock!
    let created_agg = json_req(
        &router,
        &session,
        "POST",
        "/api/orders/_aggregates/lines",
        None,
        json!({
            "parent": {
                "number": "ORD-ATOMIC-1",
                "owner_id": user_id,
                "attachment": order_file
            },
            "creates": [{
                "key": "line1",
                "input": {
                    "product_id": first_product,
                    "quantity": 2,
                    "unit_price": "19.50",
                    "currency": "CNY",
                    "spec_file": line_file
                }
            }]
        }),
    )
    .await;
    assert_eq!(created_agg.status(), StatusCode::CREATED);
    assert_eq!(created_agg.headers().get(header::ETAG).unwrap(), "\"rev-1\"");
    let agg_body = body_json(created_agg).await;
    let order_id = agg_body["parent"]["id"].as_str().unwrap().to_owned();
    assert_eq!(agg_body["rows"].as_array().unwrap().len(), 1);
    assert!(agg_body["created"]["line1"].is_string());

    for fid in [order_file, line_file] {
        assert_eq!(
            json_req(&router, &session, "DELETE", &format!("/api/files/{fid}"), None, json!({}))
                .await
                .status(),
            StatusCode::CONFLICT
        );
    }

    // Deleting parent Order cascades to OrderLine; both order_file AND cascaded line_file can then be deleted!
    let del_order = json_req(
        &router,
        &session,
        "DELETE",
        &format!("/api/orders/{order_id}"),
        Some("\"rev-1\""),
        json!({}),
    )
    .await;
    assert_eq!(del_order.status(), StatusCode::NO_CONTENT);
    for fid in [order_file, line_file] {
        assert_eq!(
            json_req(&router, &session, "DELETE", &format!("/api/files/{fid}"), None, json!({}))
                .await
                .status(),
            StatusCode::NO_CONTENT
        );
    }

    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
