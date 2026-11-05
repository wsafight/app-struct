use crate::database::{Database, DatabaseExecutor, Sql};
use crate::{ApiError, AppState};
use axum::{Json, Router, extract::State, http::HeaderMap, routing::get};
use serde::Serialize;
use sqlx::Row as _;

#[derive(Serialize)]
struct BusinessMetric {
    key: &'static str,
    value: Option<u64>,
    definition: &'static str,
    source: &'static str,
    window: &'static str,
    unavailable_reason: Option<&'static str>,
}
#[derive(Serialize)]
struct BusinessOverview {
    generated_at: chrono::DateTime<chrono::Utc>,
    metrics: Vec<BusinessMetric>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/admin/business", get(overview))
}

async fn metric(
    database: &Database,
    key: &'static str,
    query: Option<&str>,
    definition: &'static str,
    source: &'static str,
    window: &'static str,
) -> Result<BusinessMetric, ApiError> {
    let value = if let Some(query) = query {
        let row = database
            .fetch_optional(Sql::new(query.to_owned()))
            .await?
            .ok_or(ApiError::Internal)?;
        Some(u64::try_from(row.try_get::<i64, _>("total")?).unwrap_or(0))
    } else {
        None
    };
    Ok(BusinessMetric {
        key,
        value,
        definition,
        source,
        window,
        unavailable_reason: query
            .is_none()
            .then_some("The required module or event source is not configured"),
    })
}

async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BusinessOverview>, ApiError> {
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !actor.has_role("admin") {
        return Err(ApiError::Forbidden);
    }
    let metrics = vec![
        metric(&state.database, "registered_accounts", Some("SELECT COUNT(*) AS total FROM \"_appstruct_auth_accounts\""), "Registered accounts still present in this application", "Auth accounts", "Current records").await?,
        metric(&state.database, "organizations", __ORGANIZATIONS__, "Organizations created in this application", "Tenant organizations", "Current records").await?,
        metric(&state.database, "successful_reports", __REPORTS__, "Successfully completed reports still within report retention", "Report runs with stage succeeded", "Retained report history").await?,
        metric(&state.database, "activated_accounts", __FIRST_SUCCESS__, "Distinct accounts with at least one retained successful report", "Report actor IDs", "Retained report history").await?,
        metric(&state.database, "paying_organizations", __PAYING__, "Organizations whose latest subscription is active and not expired; trials are excluded", "Billing subscription state", "Current provider state").await?,
        metric(&state.database, "visits", Some("SELECT COUNT(*) AS total FROM \"_appstruct_product_events\" WHERE kind = 'visit' AND occurred_at >= CURRENT_TIMESTAMP - INTERVAL '30 days'"), "Consented page visits, deduplicated per visitor, UTC day and page; not raw pageviews", "First-party product events", "Last 30 days").await?,
        BusinessMetric { key: "revenue", value: None, definition: "Bank settlement and MRR are not inferred from subscriptions or captured charges", source: "Currency-separated Stripe receipts and refunds are available in /admin/insights", window: "Not available", unavailable_reason: Some("Bank settlement is not collected; currencies must not be summed into one number") },
    ];
    Ok(Json(BusinessOverview {
        generated_at: chrono::Utc::now(),
        metrics,
    }))
}
