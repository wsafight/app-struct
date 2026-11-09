use appstruct_generated_backend::{
    ApiError, AppState, Job, JobHandler, JobHandlerError, RequestContext,
    database::{Database, DatabaseExecutor, Sql},
    product::{notify, record_event, refund_credits, reserve_credits},
};
use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, header},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use std::{convert::Infallible, time::Duration};
use tokio::sync::OnceCell;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/product/assistant", get(list).post(submit))
        .route("/api/product/assistant/capabilities", get(capabilities))
        .route("/api/product/assistant/{id}/events", get(events))
        .route("/api/product/assistant/{id}/cancel", post(cancel))
}

#[derive(Clone, PartialEq, Eq)]
enum AiProvider {
    /// Local Ollama; plain http is allowed only for loopback.
    Ollama,
    /// OpenAI-compatible chat completions; requires HTTPS and a server-side API key.
    OpenAi,
}

struct AiConfig {
    provider: AiProvider,
    endpoint: String,
    api_key: Option<String>,
    model: String,
    max_tokens: i32,
}

fn configuration() -> Result<AiConfig, ApiError> {
    if std::env::var("APPSTRUCT_AI_ENABLED").as_deref() != Ok("true") {
        return Err(ApiError::Conflict(
            "AI is disabled; configure the optional AI adapter first".to_owned(),
        ));
    }
    let provider = match std::env::var("APPSTRUCT_AI_PROVIDER")
        .unwrap_or_else(|_| "ollama".to_owned())
        .as_str()
    {
        "ollama" => AiProvider::Ollama,
        "openai" => AiProvider::OpenAi,
        _ => return Err(ApiError::Conflict(
            "APPSTRUCT_AI_PROVIDER must be `ollama` or `openai`".to_owned(),
        )),
    };
    let default_endpoint = match provider {
        AiProvider::Ollama => "http://127.0.0.1:11434/api/generate",
        AiProvider::OpenAi => "https://api.openai.com/v1/chat/completions",
    };
    let endpoint = std::env::var("APPSTRUCT_AI_ENDPOINT")
        .unwrap_or_else(|_| default_endpoint.to_owned());
    let url = reqwest::Url::parse(&endpoint).map_err(|_| ApiError::Internal)?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    let secure_scheme = url.scheme() == "https" || (provider == AiProvider::Ollama && url.scheme() == "http" && loopback);
    if !secure_scheme
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::Internal);
    }
    // Remote providers must always use HTTPS so prompts and keys never travel in clear text.
    if provider == AiProvider::OpenAi && url.scheme() != "https" {
        return Err(ApiError::Internal);
    }
    let api_key = if provider == AiProvider::OpenAi {
        let key = std::env::var("APPSTRUCT_AI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty() && value.len() <= 512
                && !value.chars().any(char::is_whitespace))
            .ok_or_else(|| ApiError::Conflict(
                "APPSTRUCT_AI_API_KEY is required for provider `openai`".to_owned(),
            ))?;
        Some(key)
    } else {
        None
    };
    let model = std::env::var("APPSTRUCT_AI_MODEL")
        .ok()
        .filter(|name| !name.trim().is_empty() && name.len() <= 200
            && name.chars().all(|c| c.is_ascii() && !c.is_control() && c != ' '))
        .ok_or_else(|| ApiError::Conflict("APPSTRUCT_AI_MODEL is required".to_owned()))?;
    let tokens = std::env::var("APPSTRUCT_AI_MAX_TOKENS")
        .unwrap_or_else(|_| "512".to_owned())
        .parse::<i32>()
        .ok()
        .filter(|limit| (1..=4096).contains(limit))
        .ok_or(ApiError::Internal)?;
    Ok(AiConfig { provider, endpoint, api_key, model, max_tokens: tokens })
}

async fn capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    state
        .context(&headers)
        .await?
        .actor()
        .ok_or(ApiError::Unauthorized)?;
    let config = configuration();
    let provider = config
        .as_ref()
        .map(|value| match value.provider {
            AiProvider::Ollama => "ollama",
            AiProvider::OpenAi => "openai",
        })
        .unwrap_or("ollama");
    Ok(Json(
        json!({ "enabled": config.is_ok(), "provider": provider, "credit_cost": 1, "max_prompt_bytes": 8000, "max_output_bytes": 65536, "max_parallel_per_team": 2 }),
    ))
}

#[derive(Deserialize)]
struct Pagination {
    page: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<Pagination>,
) -> Result<Json<Value>, ApiError> {
    let context = state.context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let page = input.page.unwrap_or(1);
    if !(1..=10000).contains(&page) {
        return Err(ApiError::InvalidQuery("Invalid page".to_owned()));
    }
    let rows = state.database.fetch_all(Sql::bind("SELECT row_to_json(r)::jsonb AS data FROM ai_requests r WHERE tenant_id = $1 AND actor_id = $2 ORDER BY created_at DESC, id DESC LIMIT 25 OFFSET $3", [context.require_tenant()?.into(), actor.id.into(), ((page - 1) * 25).into()])).await?;
    Ok(Json(
        json!({ "data": rows.iter().map(|row| row.try_get::<Value, _>("data")).collect::<Result<Vec<_>, _>>()? }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prompt {
    prompt: String,
}

async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Prompt>,
) -> Result<Json<Value>, ApiError> {
    let config = configuration()?;
    let model = config.model;
    let max_tokens = config.max_tokens;
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    if input.prompt.trim().is_empty() || input.prompt.len() > 8000 {
        return Err(ApiError::InvalidQuery(
            "Prompt must contain 1–8000 bytes".to_owned(),
        ));
    }
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| ApiError::InvalidQuery("A UUID Idempotency-Key is required".to_owned()))?;
    let scope = format!("{tenant}:{}:{key}", actor.id);
    let transaction = state.database.begin().await?;
    transaction
        .fetch_optional(Sql::bind(
            "SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE",
            [tenant.into()],
        ))
        .await?
        .ok_or(ApiError::InvalidTenant)?;
    if let Some(row) = transaction.fetch_optional(Sql::bind("SELECT row_to_json(r)::jsonb AS data FROM ai_requests r WHERE request_key = $1 AND tenant_id = $2 AND actor_id = $3", [scope.clone().into(), tenant.into(), actor.id.into()])).await? {
        let data: Value = row.try_get("data")?;
        if data["prompt"] != input.prompt { return Err(ApiError::Conflict("Idempotency key was used for another prompt".to_owned())); }
        return Ok(Json(data));
    }
    let count = transaction.fetch_optional(Sql::bind("SELECT COUNT(*) AS total FROM ai_requests WHERE tenant_id = $1 AND state IN ('queued', 'running')", [tenant.into()])).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    if count >= 2 {
        return Err(ApiError::Conflict(
            "Two AI requests are already pending for this team".to_owned(),
        ));
    }
    let id = Uuid::now_v7();
    reserve_credits(&transaction, tenant, actor.id, id, 1).await?;
    let job_context = RequestContext::transaction_with_file(
        &transaction,
        &state.mail,
        &state.file,
        &state.realtime,
        Some(actor.clone()),
        Some(tenant),
    );
    let job = job_context
        .enqueue_job(
            "ai",
            "ai.generate",
            &json!({ "request_id": id }),
            Some(&format!("ai:{id}")),
            None,
        )
        .await
        .map_err(|_| ApiError::Internal)?;
    let row = transaction.fetch_optional(Sql::bind("INSERT INTO ai_requests (id, tenant_id, actor_id, request_key, job_id, prompt, model, max_tokens, answer, state, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, '', 'queued', CURRENT_TIMESTAMP) RETURNING row_to_json(ai_requests)::jsonb AS data", [id.into(), tenant.into(), actor.id.into(), scope.into(), job.id.into(), input.prompt.into(), model.into(), max_tokens.into()])).await?.ok_or(ApiError::Internal)?;
    transaction.commit().await?;
    Ok(Json(row.try_get("data")?))
}

async fn owned<C: DatabaseExecutor>(
    database: &C,
    tenant: Uuid,
    actor: Uuid,
    id: Uuid,
) -> Result<Value, ApiError> {
    let row = database.fetch_optional(Sql::bind("SELECT row_to_json(r)::jsonb AS data FROM ai_requests r WHERE id = $1 AND tenant_id = $2 AND actor_id = $3", [id.into(), tenant.into(), actor.into()])).await?.ok_or(ApiError::NotFound)?;
    Ok(row.try_get("data")?)
}

async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let context = state.context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?.id;
    let tenant = context.require_tenant()?;
    owned(&state.database, tenant, actor, id).await?;
    let stream = async_stream::stream! {
        let mut previous = String::new();
        for _ in 0..600 {
            let authorized = state.context(&headers).await.is_ok_and(|context| context.actor().is_some_and(|user| user.id == actor) && context.tenant() == Some(tenant));
            if !authorized { break; }
            match owned(&state.database, tenant, actor, id).await {
                Ok(value) => {
                    let current = value.to_string();
                    let terminal = !matches!(value["state"].as_str(), Some("queued" | "running"));
                    if current != previous { previous = current.clone(); yield Ok::<_, Infallible>(Event::default().event("snapshot").data(current)); }
                    if terminal { break; }
                }
                Err(_) => { yield Ok(Event::default().event("error").data("Stream unavailable; reload your history.")); break; }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };
    Ok((
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        Sse::new(stream).keep_alive(KeepAlive::default()),
    )
        .into_response())
}

async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let transaction = state.database.begin().await?;
    transaction
        .fetch_optional(Sql::bind(
            "SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE",
            [tenant.into()],
        ))
        .await?
        .ok_or(ApiError::InvalidTenant)?;
    let request = owned(&transaction, tenant, actor.id, id).await?;
    if matches!(request["state"].as_str(), Some("queued" | "running")) {
        transaction.execute(Sql::bind("UPDATE \"_appstruct_jobs\" SET status = 'cancelled', locked_by = NULL, locked_until = NULL, completed_at = CURRENT_TIMESTAMP WHERE id = (SELECT job_id FROM ai_requests WHERE id = $1 AND tenant_id = $2) AND status IN ('queued', 'running')", [id.into(), tenant.into()])).await?;
        transaction.execute(Sql::bind("UPDATE ai_requests SET state = 'cancelled', revision = revision + 1, error_code = 'AI_CANCELLED', completed_at = CURRENT_TIMESTAMP WHERE id = $1 AND tenant_id = $2", [id.into(), tenant.into()])).await?;
        refund_credits(&transaction, tenant, id).await?;
    }
    let result = owned(&transaction, tenant, actor.id, id).await?;
    transaction.commit().await?;
    Ok(Json(result))
}

#[derive(Default)]
pub struct AiJobs {
    database: OnceCell<Database>,
}

#[async_trait]
impl JobHandler for AiJobs {
    async fn handle(&self, job: &Job) -> Result<(), JobHandlerError> {
        let database = self
            .database
            .get_or_try_init(|| async {
                Database::connect(
                    std::env::var("DATABASE_URL")
                        .map_err(|_| JobHandlerError("AI_DATABASE_CONFIGURATION".to_owned()))?,
                )
                .await
                .map_err(|_| JobHandlerError("AI_DATABASE_UNAVAILABLE".to_owned()))
            })
            .await?;
        if job.kind == "ai.reconcile" {
            return reconcile(database)
                .await
                .map_err(|_| JobHandlerError("AI_RECOVERY_FAILED".to_owned()));
        }
        if job.kind != "ai.generate" {
            return Err(JobHandlerError("UNSUPPORTED_JOB_KIND".to_owned()));
        }
        let id = job.payload["request_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| JobHandlerError("AI_INVALID_JOB".to_owned()))?;
        let result = generate(database, job, id).await;
        if let Err(code) = result {
            finish(database, job, id, "failed", Some(code))
                .await
                .map_err(|_| JobHandlerError("AI_REFUND_FAILED".to_owned()))?;
        }
        Ok(())
    }
}

async fn write_answer(
    database: &Database,
    job: &Job,
    id: Uuid,
    answer: &str,
) -> Result<(), &'static str> {
    let updated = database.execute(Sql::bind("UPDATE ai_requests r SET state = 'running', revision = r.revision + 1, answer = $3 FROM \"_appstruct_jobs\" j WHERE r.id = $1 AND r.job_id = j.id AND j.id = $2 AND j.attempts = $4 AND j.status = 'running' AND j.locked_until > clock_timestamp() AND r.state IN ('queued', 'running') AND r.tenant_id IS NOT DISTINCT FROM $5", [id.into(), job.id.into(), answer.into(), job.attempts.into(), job.tenant_id.into()])).await.map_err(|_| "AI_STATE_FAILED")?;
    if updated.rows_affected() != 1 {
        return Err("AI_LEASE_LOST");
    }
    Ok(())
}

async fn generate(database: &Database, job: &Job, id: Uuid) -> Result<(), &'static str> {
    let config = configuration().map_err(|_| "AI_NOT_CONFIGURED")?;
    let row = database.fetch_optional(Sql::bind("SELECT prompt, model, max_tokens FROM ai_requests WHERE id = $1 AND job_id = $2 AND tenant_id IS NOT DISTINCT FROM $3", [id.into(), job.id.into(), job.tenant_id.into()])).await.map_err(|_| "AI_LOAD_FAILED")?.ok_or("AI_REQUEST_MISSING")?;
    let prompt: String = row.try_get("prompt").map_err(|_| "AI_LOAD_FAILED")?;
    let model: String = row.try_get("model").map_err(|_| "AI_LOAD_FAILED")?;
    let max_tokens: i32 = row.try_get("max_tokens").map_err(|_| "AI_LOAD_FAILED")?;
    write_answer(database, job, id, "").await?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "AI_PROVIDER_UNAVAILABLE")?;
    let request = match config.provider {
        AiProvider::Ollama => client.post(&config.endpoint).json(
            &json!({ "model": model, "prompt": prompt, "stream": true, "options": { "num_predict": max_tokens } }),
        ),
        AiProvider::OpenAi => client
            .post(&config.endpoint)
            .bearer_auth(config.api_key.as_deref().ok_or("AI_NOT_CONFIGURED")?)
            .json(&json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
                "max_completion_tokens": max_tokens,
                "stream": true,
            })),
    };
    let response = request.send().await.map_err(|_| "AI_PROVIDER_UNAVAILABLE")?.error_for_status().map_err(|_| "AI_PROVIDER_UNAVAILABLE")?;
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut answer = String::new();
    let mut done = false;
    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk.map_err(|_| "AI_STREAM_FAILED")?);
        if buffer.len() > 131072 {
            return Err("AI_OUTPUT_LIMIT");
        }
        while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=position).collect();
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            done = append_line(&config.provider, &line, &mut answer, max_tokens)?;
            write_answer(database, job, id, &answer).await?;
            if done {
                break;
            }
        }
        if done {
            break;
        }
    }
    if !done && !buffer.iter().all(u8::is_ascii_whitespace) {
        done = append_line(&config.provider, &buffer, &mut answer, max_tokens)?;
        write_answer(database, job, id, &answer).await?;
    }
    if !done {
        return Err("AI_STREAM_INCOMPLETE");
    }
    finish(database, job, id, "succeeded", None)
        .await
        .map_err(|_| "AI_FINALIZE_FAILED")?;
    Ok(())
}

fn append_line(provider: &AiProvider, line: &[u8], answer: &mut String, max_tokens: i32) -> Result<bool, &'static str> {
    match provider {
        AiProvider::Ollama => append_ollama_line(line, answer, max_tokens),
        AiProvider::OpenAi => append_openai_line(line, answer),
    }
}

fn append_ollama_line(line: &[u8], answer: &mut String, max_tokens: i32) -> Result<bool, &'static str> {
    let value: Value = serde_json::from_slice(line).map_err(|_| "AI_INVALID_RESPONSE")?;
    if value.get("error").is_some() {
        return Err("AI_PROVIDER_FAILED");
    }
    if let Some(text) = value["response"].as_str() {
        answer.push_str(text);
    }
    if answer.len() > 65536
        || value["eval_count"]
            .as_i64()
            .is_some_and(|count| count > i64::from(max_tokens))
    {
        return Err("AI_OUTPUT_LIMIT");
    }
    Ok(value["done"].as_bool() == Some(true))
}

/// OpenAI-compatible servers stream `data: {json}` lines terminated by `data: [DONE]`.
/// The job writes snapshots to the database, so clients still stream over the SSE endpoint.
fn append_openai_line(line: &[u8], answer: &mut String) -> Result<bool, &'static str> {
    let data = match line.strip_prefix(b"data:") {
        Some(rest) => rest,
        None => return Ok(false),
    };
    let data = std::str::from_utf8(data).map_err(|_| "AI_INVALID_RESPONSE")?.trim();
    if data == "[DONE]" {
        return Ok(true);
    }
    if data.is_empty() {
        return Ok(false);
    }
    let value: Value = serde_json::from_str(data).map_err(|_| "AI_INVALID_RESPONSE")?;
    if value.get("error").is_some() {
        return Err("AI_PROVIDER_FAILED");
    }
    for choice in value["choices"].as_array().into_iter().flatten() {
        if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str) {
            answer.push_str(text);
        }
        if answer.len() > 65536 {
            return Err("AI_OUTPUT_LIMIT");
        }
        // "stop" and "length" both terminate the completion; other reasons still finish the stream.
        if choice["finish_reason"].is_string() {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn finish(
    database: &Database,
    job: &Job,
    id: Uuid,
    stage: &str,
    code: Option<&str>,
) -> Result<(), ApiError> {
    let tenant = job.tenant_id.ok_or(ApiError::InvalidTenant)?;
    let transaction = database.begin().await?;
    transaction
        .fetch_optional(Sql::bind(
            "SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE",
            [tenant.into()],
        ))
        .await?
        .ok_or(ApiError::InvalidTenant)?;
    let row = transaction.fetch_optional(Sql::bind("UPDATE ai_requests r SET state = $3, revision = r.revision + 1, error_code = $4, completed_at = CURRENT_TIMESTAMP FROM \"_appstruct_jobs\" j WHERE r.id = $1 AND r.job_id = j.id AND j.id = $2 AND j.attempts = $5 AND j.status = 'running' AND j.locked_until > clock_timestamp() AND r.state IN ('queued', 'running') AND r.tenant_id = $6 RETURNING r.actor_id", [id.into(), job.id.into(), stage.into(), code.into(), job.attempts.into(), tenant.into()])).await?;
    if let Some(row) = row {
        let actor: Uuid = row.try_get("actor_id")?;
        if stage == "failed" {
            refund_credits(&transaction, tenant, id).await?;
        } else {
            record_event(
                &transaction,
                Some(actor),
                None,
                Some(tenant),
                &format!("ai:{id}"),
                "activation",
            )
            .await?;
        }
        notify(
            &transaction,
            actor,
            Some(tenant),
            &format!("ai:{id}:{stage}"),
            &format!("AI request {stage}"),
            "Open the AI assistant to view your result.",
            "/assistant",
        )
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

async fn reconcile(database: &Database) -> Result<(), ApiError> {
    let rows = database.fetch_all(Sql::new("SELECT r.id, r.tenant_id FROM ai_requests r LEFT JOIN \"_appstruct_jobs\" j ON j.id = r.job_id WHERE r.state IN ('queued', 'running') AND (j.id IS NULL OR j.status IN ('dead', 'cancelled', 'succeeded') OR r.created_at < CURRENT_TIMESTAMP - INTERVAL '10 minutes') ORDER BY r.created_at LIMIT 100")).await?;
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let tenant: Uuid = row.try_get("tenant_id")?;
        let transaction = database.begin().await?;
        transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
        let changed = transaction.execute(Sql::bind("UPDATE ai_requests r SET state = 'failed', revision = r.revision + 1, error_code = 'AI_INTERRUPTED', completed_at = CURRENT_TIMESTAMP WHERE r.id = $1 AND r.tenant_id = $2 AND r.state IN ('queued', 'running') AND (NOT EXISTS (SELECT 1 FROM \"_appstruct_jobs\" j WHERE j.id = r.job_id AND j.status IN ('queued', 'running')) OR r.created_at < CURRENT_TIMESTAMP - INTERVAL '10 minutes')", [id.into(), tenant.into()])).await?;
        if changed.rows_affected() == 1 {
            transaction.execute(Sql::bind("UPDATE \"_appstruct_jobs\" SET status = 'cancelled', locked_by = NULL, locked_until = NULL, completed_at = CURRENT_TIMESTAMP WHERE id = (SELECT job_id FROM ai_requests WHERE id = $1) AND status IN ('queued', 'running')", [id.into()])).await?;
            refund_credits(&transaction, tenant, id).await?;
        }
        transaction.commit().await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "assistant_tests.rs"]
mod tests;
