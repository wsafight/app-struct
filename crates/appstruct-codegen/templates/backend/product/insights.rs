use super::*;

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/product/visits", post(visit)).route("/api/product/insights", get(overview))
}

pub async fn record_event<C: DatabaseExecutor>(database: &C, actor: Option<uuid::Uuid>, visitor: Option<uuid::Uuid>, tenant: Option<uuid::Uuid>, key: &str, kind: &str) -> Result<(), ApiError> {
    record_event_with_payload(database, actor, visitor, tenant, key, kind, &json!({})).await
}

pub async fn record_event_with_payload<C: DatabaseExecutor>(database: &C, actor: Option<uuid::Uuid>, visitor: Option<uuid::Uuid>, tenant: Option<uuid::Uuid>, key: &str, kind: &str, payload: &Value) -> Result<(), ApiError> {
    database.execute(Sql::bind("INSERT INTO \"_appstruct_product_events\" (id, actor_id, visitor_id, tenant_id, event_key, kind, payload, occurred_at) VALUES ($1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP) ON CONFLICT (event_key) DO NOTHING", [uuid::Uuid::now_v7().into(), actor.into(), visitor.into(), tenant.into(), key.into(), kind.into(), payload.clone().into()])).await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Visit {
    visitor_id: uuid::Uuid,
    page: String,
    consent: bool,
    #[serde(default)]
    referrer: Option<String>,
    #[serde(default)]
    utm_source: Option<String>,
    #[serde(default)]
    utm_medium: Option<String>,
    #[serde(default)]
    utm_campaign: Option<String>,
}

fn utm(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '+')))
        .map(str::to_owned)
}

fn referrer(value: &Option<String>) -> Option<String> {
    let value = value.as_deref()?.trim();
    if value.len() > 512 { return None; }
    // Only HTTPS referrers carry a trustworthy domain; parse the host by hand so the
    // generated crate needs no URL dependency.
    let rest = value.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?
        .split('@').next_back()?
        .split(':').next()?;
    let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
    if host.is_empty() || host.len() > 100
        || !host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
    {
        return None;
    }
    Some(host)
}

fn visit_source(input: &Visit) -> (String, Option<String>, Option<String>) {
    if let Some(source) = utm(&input.utm_source) {
        return (source, utm(&input.utm_medium), utm(&input.utm_campaign));
    }
    match referrer(&input.referrer) {
        Some(domain) => (domain, Some("referral".to_owned()), None),
        None => ("direct".to_owned(), None, None),
    }
}

async fn visit(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Visit>) -> Result<StatusCode, ApiError> {
    state.auth.validate_origin(&headers)?;
    if !input.consent || !matches!(input.page.as_str(), "/" | "/pricing") { return Err(invalid("Only consented public-page visits are accepted")); }
    let key = format!("visit:{}:{}:{}", input.visitor_id, chrono::Utc::now().format("%Y-%m-%d"), input.page);
    let (source, medium, campaign) = visit_source(&input);
    let payload = json!({ "source": source, "medium": medium, "campaign": campaign });
    let transaction = state.database.begin().await?;
    transaction.fetch_optional(Sql::new("SELECT pg_advisory_xact_lock(77146, 1)")).await?;
    let count = transaction.fetch_optional(Sql::new("SELECT COUNT(*) AS total FROM \"_appstruct_product_events\" WHERE kind = 'visit' AND occurred_at >= (date_trunc('day', CURRENT_TIMESTAMP AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')")).await?.ok_or(ApiError::Internal)?.try_get::<i64, _>("total")?;
    if count >= setting("APPSTRUCT_DAILY_VISIT_EVENT_LIMIT", 100000, 10000000)? { return Err(ApiError::TooManyRequests); }
    record_event_with_payload(&transaction, None, Some(input.visitor_id), None, &key, "visit", &payload).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let actor = state.auth.actor(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    if !actor.has_role("admin") { return Err(ApiError::Forbidden); }
    let row = state.database.fetch_optional(Sql::new("SELECT COUNT(DISTINCT visitor_id) FILTER (WHERE kind = 'visit') AS visitors, COUNT(DISTINCT actor_id) FILTER (WHERE kind = 'signup') AS signups, COUNT(DISTINCT actor_id) FILTER (WHERE kind = 'activation') AS activated FROM \"_appstruct_product_events\" WHERE occurred_at >= CURRENT_TIMESTAMP - INTERVAL '30 days'")).await?.ok_or(ApiError::Internal)?;
    let retention = state.database.fetch_optional(Sql::new("WITH cohort AS (SELECT actor_id, MIN(occurred_at) AS started FROM \"_appstruct_product_events\" WHERE kind = 'activation' AND actor_id IS NOT NULL GROUP BY actor_id), eligible AS (SELECT * FROM cohort WHERE started >= CURRENT_TIMESTAMP - INTERVAL '30 days' AND started < CURRENT_TIMESTAMP - INTERVAL '8 days') SELECT COUNT(*) AS eligible, COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM \"_appstruct_product_events\" e WHERE e.actor_id = eligible.actor_id AND e.kind = 'activation' AND e.occurred_at >= eligible.started + INTERVAL '7 days' AND e.occurred_at < eligible.started + INTERVAL '8 days')) AS returned FROM eligible")).await?.ok_or(ApiError::Internal)?;
    let revenue = state.database.fetch_all(Sql::new("SELECT currency, SUM(captured)::text AS captured, SUM(refunded)::text AS refunded, SUM(captured - refunded)::text AS net, COUNT(DISTINCT tenant_id) FILTER (WHERE captured > refunded) AS paying_organizations FROM \"_appstruct_product_payments\" WHERE paid_at >= CURRENT_TIMESTAMP - INTERVAL '30 days' GROUP BY currency ORDER BY currency")).await?;
    let revenue = revenue.iter().map(|row| Ok(json!({ "currency": row.try_get::<String, _>("currency")?, "captured_minor": row.try_get::<String, _>("captured")?, "refunded_minor": row.try_get::<String, _>("refunded")?, "net_minor": row.try_get::<String, _>("net")?, "paying_organizations": row.try_get::<i64, _>("paying_organizations")? }))).collect::<Result<Vec<Value>, ApiError>>()?;
    let sources = state.database.fetch_all(Sql::new("SELECT COALESCE(payload->>'source', 'unknown') AS source, payload->>'medium' AS medium, COUNT(DISTINCT visitor_id) AS visitors FROM \"_appstruct_product_events\" WHERE kind = 'visit' AND occurred_at >= CURRENT_TIMESTAMP - INTERVAL '30 days' GROUP BY 1, 2 ORDER BY visitors DESC LIMIT 20")).await?;
    let sources = sources.iter().map(|row| Ok(json!({ "source": row.try_get::<String, _>("source")?, "medium": row.try_get::<Option<String>, _>("medium")?, "visitors": row.try_get::<i64, _>("visitors")? }))).collect::<Result<Vec<Value>, ApiError>>()?;
    let stripe_configured = std::env::var("APPSTRUCT_STRIPE_SECRET_KEY").is_ok_and(|value| !value.trim().is_empty()) && std::env::var("APPSTRUCT_STRIPE_WEBHOOK_SECRET").is_ok_and(|value| !value.trim().is_empty());
    let polar_configured = std::env::var("APPSTRUCT_POLAR_ACCESS_TOKEN").is_ok_and(|value| !value.trim().is_empty()) && std::env::var("APPSTRUCT_POLAR_WEBHOOK_SECRET").is_ok_and(|value| !value.trim().is_empty());
    let revenue_available = __BILLING__ && (stripe_configured || polar_configured);
    Ok(Json(json!({ "generated_at": chrono::Utc::now(), "window_days": 30, "visitors": row.try_get::<i64, _>("visitors")?, "signups": row.try_get::<i64, _>("signups")?, "activated": row.try_get::<i64, _>("activated")?, "retention_d7": { "eligible": retention.try_get::<i64, _>("eligible")?, "returned": retention.try_get::<i64, _>("returned")? }, "revenue": revenue, "revenue_available": revenue_available, "sources": sources, "definitions": { "visitors": "Consented first-party visitor IDs, deduplicated by UTC day/page; may include bots", "signups": "Server-recorded registrations", "activated": "Distinct actors completing business work", "retention_d7": "First activation cohort from the last 30 days, observed at age 7–8 days; only fully observed cohorts", "revenue": "Captured payments minus recorded refunds, grouped by currency, in minor units; before fees, not bank settlement or MRR", "sources": "Visitor sources derived from consented UTM parameters or the HTTPS referrer domain; visits without either count as direct", "funnel": "Window counts, not an attributed cohort conversion rate" } })))
}
