use appstruct_generated_backend::{
    ApiError, AppState, RequestContext,
    database::{DatabaseExecutor, Sql},
    product::notify,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/product/approvals", get(list).post(submit))
        .route("/api/product/approvals/{id}/decide", post(decide))
        .route("/api/product/approvals/{id}/history", get(history))
        .route("/api/product/approvals/{id}/report", get(report))
}

#[derive(Deserialize)]
struct ListInput {
    page: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<ListInput>,
) -> Result<Json<Value>, ApiError> {
    let context = state.context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let page = input.page.unwrap_or(1);
    if !(1..=10000).contains(&page) {
        return Err(ApiError::InvalidQuery("Invalid page".to_owned()));
    }
    let rows = state.database.fetch_all(Sql::bind("SELECT row_to_json(r)::jsonb AS data FROM approval_requests r WHERE tenant_id = $1 AND (requested_by = $2 OR reviewer_id = $2) ORDER BY created_at DESC, id DESC LIMIT 25 OFFSET $3", [tenant.into(), actor.id.into(), ((page - 1) * 25).into()])).await?;
    let data = rows
        .iter()
        .map(|row| row.try_get::<Value, _>("data"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submit {
    task_id: Uuid,
    report_run_id: Uuid,
    reviewer_id: Uuid,
    note: String,
}

async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Submit>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    if input.reviewer_id == actor.id || input.note.len() > 2000 {
        return Err(ApiError::InvalidQuery(
            "Choose another reviewer; notes are limited to 2000 bytes".to_owned(),
        ));
    }
    let id = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| ApiError::InvalidQuery("A UUID Idempotency-Key is required".to_owned()))?;
    let transaction = state.database.begin().await?;
    transaction
        .fetch_optional(Sql::bind(
            "SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE",
            [tenant.into()],
        ))
        .await?
        .ok_or(ApiError::InvalidTenant)?;
    if let Some(row) = transaction.fetch_optional(Sql::bind("SELECT row_to_json(r)::jsonb AS data FROM approval_requests r WHERE id = $1 AND tenant_id = $2 AND requested_by = $3", [id.into(), tenant.into(), actor.id.into()])).await? {
        let data: Value = row.try_get("data")?;
        if data["task_id"] != input.task_id.to_string() || data["report_run_id"] != input.report_run_id.to_string() || data["reviewer_id"] != input.reviewer_id.to_string() || data["note"] != input.note { return Err(ApiError::Conflict("Idempotency key was used for different approval input".to_owned())); }
        return Ok((StatusCode::OK, Json(data)));
    }
    transaction.fetch_optional(Sql::bind("SELECT 1 FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2", [tenant.into(), input.reviewer_id.into()])).await?.ok_or(ApiError::NotFound)?;
    let task = transaction.fetch_optional(Sql::bind("SELECT row_to_json(t)::jsonb AS snapshot FROM tasks t WHERE id = $1 AND tenant_id = $2 FOR UPDATE", [input.task_id.into(), tenant.into()])).await?.ok_or(ApiError::NotFound)?;
    let snapshot: Value = task.try_get("snapshot")?;
    if snapshot["status"] == "done" {
        return Err(ApiError::Conflict(
            "This task is already complete".to_owned(),
        ));
    }
    transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_report_runs\" WHERE id = $1 AND tenant_id = $2 AND actor_id = $3 AND stage = 'succeeded' AND expires_at > CURRENT_TIMESTAMP", [input.report_run_id.into(), tenant.into(), actor.id.into()])).await?.ok_or(ApiError::NotFound)?;
    if transaction.fetch_optional(Sql::bind("SELECT 1 FROM approval_requests WHERE task_id = $1 AND tenant_id = $2 AND state = 'pending'", [input.task_id.into(), tenant.into()])).await?.is_some() { return Err(ApiError::Conflict("This task already has a pending approval".to_owned())); }
    let row = transaction.fetch_optional(Sql::bind("INSERT INTO approval_requests (id, tenant_id, task_id, report_run_id, requested_by, reviewer_id, task_snapshot, state, note, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, CURRENT_TIMESTAMP) RETURNING row_to_json(approval_requests)::jsonb AS data", [id.into(), tenant.into(), input.task_id.into(), input.report_run_id.into(), actor.id.into(), input.reviewer_id.into(), snapshot.into(), input.note.clone().into()])).await?.ok_or(ApiError::Internal)?;
    transaction.execute(Sql::bind("INSERT INTO approval_decisions (id, tenant_id, request_id, requested_by, reviewer_id, actor_id, decision, note, created_at) VALUES ($1, $2, $3, $4, $5, $4, 'submitted', $6, CURRENT_TIMESTAMP)", [Uuid::now_v7().into(), tenant.into(), id.into(), actor.id.into(), input.reviewer_id.into(), input.note.into()])).await?;
    notify(
        &transaction,
        input.reviewer_id,
        Some(tenant),
        &format!("approval:{id}:submitted"),
        "Approval requested",
        "Review the task snapshot and supporting report.",
        "/approvals",
    )
    .await?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(row.try_get("data")?)))
}

async fn owned<C: DatabaseExecutor>(
    database: &C,
    context: &RequestContext<'_>,
    id: Uuid,
    locked: bool,
) -> Result<Value, ApiError> {
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let query = format!(
        "SELECT row_to_json(r)::jsonb AS data FROM approval_requests r WHERE id = $1 AND tenant_id = $2 AND (requested_by = $3 OR reviewer_id = $3){}",
        if locked { " FOR UPDATE" } else { "" }
    );
    let row = database
        .fetch_optional(Sql::bind(
            query,
            [id.into(), tenant.into(), actor.id.into()],
        ))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(row.try_get("data")?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    decision: String,
    note: String,
}

async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<Decision>,
) -> Result<Json<Value>, ApiError> {
    let context = state.mutation_context(&headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    if !matches!(
        input.decision.as_str(),
        "approved" | "rejected" | "cancelled"
    ) || input.note.len() > 2000
    {
        return Err(ApiError::InvalidQuery(
            "Invalid decision or note".to_owned(),
        ));
    }
    let transaction = state.database.begin().await?;
    transaction
        .fetch_optional(Sql::bind(
            "SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE",
            [tenant.into()],
        ))
        .await?
        .ok_or(ApiError::InvalidTenant)?;
    let request = owned(&transaction, &context, id, true).await?;
    let requester = parse_id(&request, "requested_by")?;
    let reviewer = parse_id(&request, "reviewer_id")?;
    if input.decision == "cancelled" && actor.id != requester
        || input.decision != "cancelled" && actor.id != reviewer
    {
        return Err(ApiError::Forbidden);
    }
    if request["state"] != "pending" {
        let previous = transaction.fetch_optional(Sql::bind("SELECT 1 FROM approval_decisions WHERE request_id = $1 AND tenant_id = $2 AND actor_id = $3 AND decision = $4 AND note = $5", [id.into(), tenant.into(), actor.id.into(), input.decision.into(), input.note.into()])).await?;
        return if previous.is_some() {
            Ok(Json(request))
        } else {
            Err(ApiError::Conflict(
                "This approval is already closed".to_owned(),
            ))
        };
    }
    let task_id = parse_id(&request, "task_id")?;
    let task = transaction.fetch_optional(Sql::bind("SELECT row_to_json(t)::jsonb AS snapshot FROM tasks t WHERE id = $1 AND tenant_id = $2 FOR UPDATE", [task_id.into(), tenant.into()])).await?.ok_or(ApiError::NotFound)?;
    if input.decision != "cancelled"
        && task.try_get::<Value, _>("snapshot")? != request["task_snapshot"]
    {
        return Err(ApiError::Conflict(
            "The task changed; cancel and resubmit the approval".to_owned(),
        ));
    }
    if input.decision == "approved" {
        transaction.execute(Sql::bind("UPDATE tasks SET status = 'done', revision = revision + 1 WHERE id = $1 AND tenant_id = $2", [task_id.into(), tenant.into()])).await?;
        let after = transaction.fetch_optional(Sql::bind("SELECT row_to_json(t)::jsonb AS data FROM tasks t WHERE id = $1 AND tenant_id = $2", [task_id.into(), tenant.into()])).await?.ok_or(ApiError::NotFound)?.try_get::<Value, _>("data")?;
        appstruct_generated_backend::audit::record(
            &transaction,
            &context,
            "app::Task",
            task_id.to_string(),
            "approval.complete",
            Some(&request["task_snapshot"]),
            Some(&after),
        )
        .await?;
    }
    transaction.execute(Sql::bind("UPDATE approval_requests SET state = $3, revision = revision + 1, completed_at = CURRENT_TIMESTAMP WHERE id = $1 AND tenant_id = $2", [id.into(), tenant.into(), input.decision.clone().into()])).await?;
    transaction.execute(Sql::bind("INSERT INTO approval_decisions (id, tenant_id, request_id, requested_by, reviewer_id, actor_id, decision, note, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP)", [Uuid::now_v7().into(), tenant.into(), id.into(), requester.into(), reviewer.into(), actor.id.into(), input.decision.clone().into(), input.note.into()])).await?;
    let recipient = if actor.id == requester {
        reviewer
    } else {
        requester
    };
    notify(
        &transaction,
        recipient,
        Some(tenant),
        &format!("approval:{id}:{}", input.decision),
        "Approval updated",
        &input.decision,
        "/approvals",
    )
    .await?;
    let after = owned(&transaction, &context, id, false).await?;
    transaction.commit().await?;
    Ok(Json(after))
}

fn parse_id(value: &Value, key: &str) -> Result<Uuid, ApiError> {
    value[key]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(ApiError::Internal)
}

async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let context = state.context(&headers).await?;
    owned(&state.database, &context, id, false).await?;
    let rows = state.database.fetch_all(Sql::bind("SELECT row_to_json(d)::jsonb AS data FROM approval_decisions d WHERE request_id = $1 AND tenant_id = $2 ORDER BY created_at, id LIMIT 100", [id.into(), context.require_tenant()?.into()])).await?;
    Ok(Json(
        json!({ "data": rows.iter().map(|row| row.try_get::<Value, _>("data")).collect::<Result<Vec<_>, _>>()? }),
    ))
}

async fn report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let context = state.context(&headers).await?;
    let request = owned(&state.database, &context, id, false).await?;
    let row = state.database.fetch_optional(Sql::bind("SELECT result_object_key FROM \"_appstruct_report_runs\" WHERE id = $1 AND tenant_id = $2 AND stage = 'succeeded' AND expires_at > CURRENT_TIMESTAMP", [parse_id(&request, "report_run_id")?.into(), context.require_tenant()?.into()])).await?.ok_or(ApiError::NotFound)?;
    let key = row
        .try_get::<Option<String>, _>("result_object_key")?
        .ok_or(ApiError::NotFound)?;
    let (_, bytes) = state
        .file
        .get(&key, context.tenant())
        .await
        .map_err(|_| ApiError::NotFound)?;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=approval-report.pdf",
        )
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(bytes))
        .map_err(|_| ApiError::Internal)
}
