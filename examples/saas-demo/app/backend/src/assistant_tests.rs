use super::*;
use crate::tests::{body, request, setup, team, user};
use appstruct_generated_backend::JobWorker;
use axum::{body::Body, http::StatusCode, routing::post};
use std::sync::Arc;

#[test]
fn ndjson_limits_errors_and_final_line() {
    let mut answer = String::new();
    assert!(!append_line(br#"{"response":"Hello ","done":false}"#, &mut answer, 20).unwrap());
    assert!(
        append_line(
            br#"{"response":"world","done":true,"eval_count":2}"#,
            &mut answer,
            20
        )
        .unwrap()
    );
    assert_eq!(answer, "Hello world");
    assert_eq!(
        append_line(br#"{"error":"model failed"}"#, &mut answer, 20),
        Err("AI_PROVIDER_FAILED")
    );
    assert_eq!(
        append_line(b"not json", &mut answer, 20),
        Err("AI_INVALID_RESPONSE")
    );
    assert_eq!(
        append_line(br#"{"done":true,"eval_count":21}"#, &mut answer, 20),
        Err("AI_OUTPUT_LIMIT")
    );
    answer = "x".repeat(65536);
    assert_eq!(
        append_line(br#"{"response":"x"}"#, &mut answer, 20),
        Err("AI_OUTPUT_LIMIT")
    );
}

#[tokio::test]
async fn queued_ai_streaming_idempotency_failure_cancellation_and_recovery() {
    if std::env::var("APPSTRUCT_AI_ENABLED").as_deref() != Ok("true") {
        return;
    }
    let Some((root, database, schema, router, _)) = setup().await else {
        return;
    };
    let endpoint = std::env::var("APPSTRUCT_AI_ENDPOINT").unwrap();
    let url = reqwest::Url::parse(&endpoint).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", url.port().unwrap()))
        .await
        .unwrap();
    let stub = Router::new().route(
        "/api/generate",
        post(|Json(input): Json<Value>| async move {
            let prompt = input["prompt"].as_str().unwrap_or("");
            let chunks = match prompt {
                "failure" => vec!["{\"error\":\"stub failure\"}"],
                "incomplete" => vec!["{\"response\":\"partial\",\"done\":false}\n"],
                _ => vec![
                    "{\"res",
                    "ponse\":\"Hello \",\"done\":false}\n{\"response\":\"world\",\"done\":true}",
                ],
            };
            Body::from_stream(futures_util::stream::iter(
                chunks.into_iter().map(Ok::<_, Infallible>),
            ))
        }),
    );
    let service = tokio::spawn(async move {
        axum::serve(listener, stub).await.unwrap();
    });
    let (mut session, _) = user(&router, "ai@example.test").await;
    team(&router, &mut session).await;
    let handler = AiJobs {
        database: OnceCell::from(database.clone()),
    };
    let worker = JobWorker::for_kind(database.clone(), Arc::new(handler), "ai.generate");
    let initial = body(
        request(
            &router,
            &session,
            "GET",
            "/api/product/credits",
            json!({}),
            "",
        )
        .await,
    )
    .await["available"]
        .as_i64()
        .unwrap();
    for (prompt, expected) in [
        ("success", "succeeded"),
        ("failure", "failed"),
        ("incomplete", "failed"),
    ] {
        let key = Uuid::now_v7().to_string();
        let response = request(
            &router,
            &session,
            "POST",
            "/api/product/assistant",
            json!({ "prompt": prompt }),
            &key,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let value = body(response).await;
        let repeated = body(
            request(
                &router,
                &session,
                "POST",
                "/api/product/assistant",
                json!({ "prompt": prompt }),
                &key,
            )
            .await,
        )
        .await;
        assert_eq!(value["id"], repeated["id"]);
        assert_eq!(
            request(
                &router,
                &session,
                "POST",
                "/api/product/assistant",
                json!({ "prompt": "different" }),
                &key
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        assert!(worker.run_once().await.unwrap());
        let history = body(
            request(
                &router,
                &session,
                "GET",
                "/api/product/assistant",
                json!({}),
                "",
            )
            .await,
        )
        .await;
        let result = history["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == value["id"])
            .unwrap();
        assert_eq!(result["state"], expected);
        if expected == "succeeded" {
            assert_eq!(result["answer"], "Hello world");
        }
        let stream = request(
            &router,
            &session,
            "GET",
            &format!(
                "/api/product/assistant/{}/events",
                value["id"].as_str().unwrap()
            ),
            json!({}),
            "",
        )
        .await;
        assert_eq!(stream.headers()[header::CACHE_CONTROL], "private, no-store");
        let data = axum::body::to_bytes(stream.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&data).contains(expected));
    }
    for _ in 0..2 {
        let value = body(
            request(
                &router,
                &session,
                "POST",
                "/api/product/assistant",
                json!({ "prompt": "cancel" }),
                &Uuid::now_v7().to_string(),
            )
            .await,
        )
        .await;
        for _ in 0..2 {
            assert_eq!(
                request(
                    &router,
                    &session,
                    "POST",
                    &format!(
                        "/api/product/assistant/{}/cancel",
                        value["id"].as_str().unwrap()
                    ),
                    json!({}),
                    ""
                )
                .await
                .status(),
                StatusCode::OK
            );
        }
    }
    let value = body(
        request(
            &router,
            &session,
            "POST",
            "/api/product/assistant",
            json!({ "prompt": "interrupted" }),
            &Uuid::now_v7().to_string(),
        )
        .await,
    )
    .await;
    database
        .execute(Sql::bind(
            "UPDATE \"_appstruct_jobs\" SET status = 'dead' WHERE id = $1",
            [Uuid::parse_str(value["job_id"].as_str().unwrap())
                .unwrap()
                .into()],
        ))
        .await
        .unwrap();
    reconcile(&database).await.unwrap();
    reconcile(&database).await.unwrap();
    let credits = body(
        request(
            &router,
            &session,
            "GET",
            "/api/product/credits",
            json!({}),
            "",
        )
        .await,
    )
    .await;
    assert_eq!(credits["available"], initial - 1);
    let (mut other, _) = user(&router, "other-ai@example.test").await;
    team(&router, &mut other).await;
    assert_eq!(
        request(
            &router,
            &other,
            "POST",
            &format!(
                "/api/product/assistant/{}/cancel",
                value["id"].as_str().unwrap()
            ),
            json!({}),
            ""
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &router,
            &other,
            "GET",
            &format!(
                "/api/product/assistant/{}/events",
                value["id"].as_str().unwrap()
            ),
            json!({}),
            ""
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    service.abort();
    root.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
