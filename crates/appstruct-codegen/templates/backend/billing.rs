use crate::{ApiError, AppState};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::time::Duration;

const TRIALS_ENABLED: bool = __TRIALS__;
const CUSTOMER_PORTAL_ENABLED: bool = __CUSTOMER_PORTAL__;
const STRIPE_API: &str = "https://api.stripe.com/v1";
const PLANS: &[Plan] = &[
    __PLANS__
];

struct Plan {
    id: &'static str,
    price_env: &'static str,
    trial_days: Option<u16>,
    entitlements: &'static [&'static str],
}

struct BillingConfig {
    secret_key: String,
    webhook_secret: String,
    prices: Vec<(&'static str, String)>,
}

#[derive(Serialize)]
struct PlanView {
    id: &'static str,
    trial_days: Option<u16>,
    entitlements: &'static [&'static str],
}

#[derive(Serialize)]
struct SubscriptionView {
    plan_id: String,
    status: String,
    current_period_end: Option<chrono::DateTime<chrono::Utc>>,
    cancel_at_period_end: bool,
}

#[derive(Serialize)]
struct BillingOverview {
    plans: Vec<PlanView>,
    subscription: Option<SubscriptionView>,
    entitlements: Vec<&'static str>,
    customer_portal: bool,
}

#[derive(Deserialize)]
struct CheckoutInput { plan_id: String }

#[derive(Serialize)]
struct RedirectUrl { url: String }

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/billing", get(overview))
        .route("/api/billing/checkout", post(checkout))
        .route("/api/billing/portal", post(portal))
        .route("/api/billing/stripe/webhook", post(webhook))
        .layer(DefaultBodyLimit::max(65_536))
}

pub fn validate_env() -> Result<(), String> {
    let _ = config_from_env()?;
    Ok(())
}

fn config_from_env() -> Result<BillingConfig, String> {
    let required = |name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("{name} is required when Stripe Billing is enabled"))
    };
    let secret_key = required("APPSTRUCT_STRIPE_SECRET_KEY")?;
    let webhook_secret = required("APPSTRUCT_STRIPE_WEBHOOK_SECRET")?;
    if !secret_key.starts_with("sk_") || !webhook_secret.starts_with("whsec_") {
        return Err("Stripe Billing credentials have invalid prefixes".to_owned());
    }
    let prices = PLANS
        .iter()
        .map(|plan| {
            let price = required(plan.price_env)?;
            if !price.starts_with("price_") {
                return Err(format!("{} must be a Stripe Price ID", plan.price_env));
            }
            Ok((plan.id, price))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(BillingConfig { secret_key, webhook_secret, prices })
}

fn config() -> Result<BillingConfig, ApiError> {
    config_from_env().map_err(|error| {
        tracing::error!(%error, "billing configuration is invalid");
        ApiError::Internal
    })
}

async fn overview(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<Json<BillingOverview>, ApiError> {
    let context = state.context(&headers).await?;
    context.actor().ok_or(ApiError::Unauthorized)?;
    let organization_id = context.require_tenant()?;
    let subscription = current_subscription(&state, organization_id).await?;
    let entitlements = subscription.as_ref().and_then(|subscription| {
        if matches!(subscription.status.as_str(), "active" | "trialing") {
            PLANS.iter().find(|plan| plan.id == subscription.plan_id)
        } else {
            None
        }
    }).map_or_else(Vec::new, |plan| plan.entitlements.to_vec());
    Ok(Json(BillingOverview {
        plans: PLANS.iter().map(|plan| PlanView {
            id: plan.id, trial_days: plan.trial_days, entitlements: plan.entitlements,
        }).collect(),
        subscription,
        entitlements,
        customer_portal: CUSTOMER_PORTAL_ENABLED,
    }))
}

async fn checkout(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<CheckoutInput>,
) -> Result<Json<RedirectUrl>, ApiError> {
    let (organization_id, email) = require_owner(&state, &headers).await?;
    if let Some(subscription) = current_subscription(&state, organization_id).await?
        && matches!(subscription.status.as_str(), "active" | "trialing" | "past_due")
    {
        return Err(ApiError::Conflict(
            "An active subscription must be changed through the customer portal".to_owned(),
        ));
    }
    let plan = PLANS.iter().find(|plan| plan.id == input.plan_id)
        .ok_or_else(|| ApiError::InvalidQuery("Unknown billing plan".to_owned()))?;
    let config = config()?;
    let price = config.prices.iter().find(|(id, _)| *id == plan.id)
        .map(|(_, price)| price.as_str()).ok_or(ApiError::Internal)?;
    let customer = customer_id(&state, organization_id).await?;
    let customer = match customer {
        Some(id) => id,
        None => create_customer(&state, &config, organization_id, &email).await?,
    };
    let base_url = state.auth.config.frontend_url.trim_end_matches('/');
    let mut form = vec![
        ("mode", "subscription".to_owned()),
        ("customer", customer),
        ("line_items[0][price]", price.to_owned()),
        ("line_items[0][quantity]", "1".to_owned()),
        ("success_url", format!("{base_url}/billing?checkout=success")),
        ("cancel_url", format!("{base_url}/billing?checkout=cancel")),
        ("client_reference_id", organization_id.to_string()),
        ("subscription_data[metadata][appstruct_organization_id]", organization_id.to_string()),
        ("subscription_data[metadata][appstruct_plan_id]", plan.id.to_owned()),
    ];
    if TRIALS_ENABLED {
        if let Some(days) = plan.trial_days {
            form.push(("subscription_data[trial_period_days]", days.to_string()));
        }
    }
    let request_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 255)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("appstruct-checkout-{organization_id}-{}", uuid::Uuid::now_v7()));
    let result = stripe_post(&config, "/checkout/sessions", &form, Some(&request_key)).await?;
    let url = result.get("url").and_then(Value::as_str).ok_or(ApiError::Internal)?;
    Ok(Json(RedirectUrl { url: url.to_owned() }))
}

async fn portal(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<Json<RedirectUrl>, ApiError> {
    if !CUSTOMER_PORTAL_ENABLED { return Err(ApiError::NotFound); }
    let (organization_id, _) = require_owner(&state, &headers).await?;
    let customer = customer_id(&state, organization_id).await?.ok_or(ApiError::NotFound)?;
    let base_url = state.auth.config.frontend_url.trim_end_matches('/');
    let result = stripe_post(&config()?, "/billing_portal/sessions", &[
        ("customer", customer),
        ("return_url", format!("{base_url}/billing")),
    ], None).await?;
    let url = result.get("url").and_then(Value::as_str).ok_or(ApiError::Internal)?;
    Ok(Json(RedirectUrl { url: url.to_owned() }))
}

async fn webhook(headers: HeaderMap, State(state): State<AppState>, body: Bytes) -> Result<StatusCode, ApiError> {
    let config = config()?;
    verify_signature(&headers, &body, &config.webhook_secret)?;
    let event: Value = serde_json::from_slice(&body).map_err(|_| ApiError::InvalidQuery("Invalid Stripe event".to_owned()))?;
    let event_id = event.get("id").and_then(Value::as_str).filter(|id| id.starts_with("evt_"))
        .ok_or_else(|| ApiError::InvalidQuery("Invalid Stripe event ID".to_owned()))?;
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
    let subscription_id = match event_type {
        "customer.subscription.created" | "customer.subscription.updated" | "customer.subscription.deleted" =>
            event.pointer("/data/object/id").and_then(Value::as_str),
        "checkout.session.completed" => event.pointer("/data/object/subscription").and_then(Value::as_str),
        _ => return Ok(StatusCode::NO_CONTENT),
    }.filter(|id| id.starts_with("sub_")).ok_or_else(|| ApiError::InvalidQuery("Missing Stripe subscription ID".to_owned()))?;
    reconcile_subscription(&state, &config, event_id, event_type, subscription_id, &event).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> Result<(), ApiError> {
    let signature = headers.get("stripe-signature").and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Forbidden)?;
    let timestamp = signature.split(',').find_map(|item| item.strip_prefix("t=")?.parse::<i64>().ok())
        .ok_or(ApiError::Forbidden)?;
    if (chrono::Utc::now().timestamp() - timestamp).abs() > 300 { return Err(ApiError::Forbidden); }
    let payload = format!("{timestamp}.{}", String::from_utf8_lossy(body));
    let verified = signature.split(',').filter_map(|item| item.strip_prefix("v1="))
        .filter_map(|hex| decode_hex(hex).ok())
        .any(|expected| {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
            mac.update(payload.as_bytes());
            mac.verify_slice(&expected).is_ok()
        });
    if verified { Ok(()) } else { Err(ApiError::Forbidden) }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ()> {
    if value.len() != 64 { return Err(()); }
    value.as_bytes().chunks_exact(2).map(|chunk| {
        let high = (chunk[0] as char).to_digit(16).ok_or(())?;
        let low = (chunk[1] as char).to_digit(16).ok_or(())?;
        Ok(((high << 4) | low) as u8)
    }).collect()
}

async fn require_owner(state: &AppState, headers: &HeaderMap) -> Result<(uuid::Uuid, String), ApiError> {
    let context = state.mutation_context(headers).await?;
    let actor = context.actor().ok_or(ApiError::Unauthorized)?;
    let organization_id = context.require_tenant()?;
    let owner = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT 1 FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2 AND role = 'owner'",
        [organization_id.into(), actor.id.into()],
    )).await?.is_some();
    if !owner { return Err(ApiError::Forbidden); }
    Ok((organization_id, actor.email.clone()))
}

async fn current_subscription(state: &AppState, organization_id: uuid::Uuid) -> Result<Option<SubscriptionView>, ApiError> {
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT plan_id, status, current_period_end, cancel_at_period_end FROM \"_appstruct_billing_subscriptions\" WHERE organization_id = $1 ORDER BY updated_at DESC, id DESC LIMIT 1",
        [organization_id.into()],
    )).await?;
    row.map(|row| -> Result<SubscriptionView, sea_orm::DbErr> { Ok(SubscriptionView {
        plan_id: row.try_get("", "plan_id")?,
        status: row.try_get("", "status")?,
        current_period_end: row.try_get("", "current_period_end")?,
        cancel_at_period_end: row.try_get("", "cancel_at_period_end")?,
    }) }).transpose().map_err(Into::into)
}

async fn customer_id(state: &AppState, organization_id: uuid::Uuid) -> Result<Option<String>, ApiError> {
    let row = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT provider_customer_id FROM \"_appstruct_billing_customers\" WHERE organization_id = $1",
        [organization_id.into()],
    )).await?;
    row.map(|row| row.try_get("", "provider_customer_id")).transpose().map_err(Into::into)
}

async fn create_customer(state: &AppState, config: &BillingConfig, organization_id: uuid::Uuid, email: &str) -> Result<String, ApiError> {
    let result = stripe_post(config, "/customers", &[
        ("email", email.to_owned()),
        ("metadata[appstruct_organization_id]", organization_id.to_string()),
    ], Some(&format!("appstruct-customer-{organization_id}"))).await?;
    let customer = result.get("id").and_then(Value::as_str).filter(|id| id.starts_with("cus_"))
        .ok_or(ApiError::Internal)?;
    state.database.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_billing_customers\" (id, organization_id, provider_customer_id, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP) ON CONFLICT (organization_id) DO NOTHING",
        [uuid::Uuid::now_v7().into(), organization_id.into(), customer.into()],
    )).await?;
    customer_id(state, organization_id).await?.ok_or(ApiError::Internal)
}

async fn stripe_post(config: &BillingConfig, path: &str, form: &[(&str, String)], idempotency_key: Option<&str>) -> Result<Value, ApiError> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|_| ApiError::Internal)?;
    let mut request = client.post(format!("{STRIPE_API}{path}"))
        .bearer_auth(&config.secret_key).form(form);
    if let Some(key) = idempotency_key { request = request.header("Idempotency-Key", key); }
    let response = request.send().await.map_err(|_| ApiError::Internal)?;
    if !response.status().is_success() { return Err(ApiError::Internal); }
    response.json().await.map_err(|_| ApiError::Internal)
}

async fn stripe_subscription(config: &BillingConfig, subscription_id: &str) -> Result<Value, ApiError> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|_| ApiError::Internal)?;
    let response = client.get(format!("{STRIPE_API}/subscriptions/{subscription_id}"))
        .bearer_auth(&config.secret_key).send().await.map_err(|_| ApiError::Internal)?;
    if !response.status().is_success() { return Err(ApiError::Internal); }
    response.json().await.map_err(|_| ApiError::Internal)
}

async fn reconcile_subscription(
    state: &AppState, config: &BillingConfig, event_id: &str, event_type: &str,
    subscription_id: &str, event: &Value,
) -> Result<(), ApiError> {
    let transaction = state.database.begin().await?;
    transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT pg_advisory_xact_lock(77145, hashtext($1))",
        [subscription_id.into()],
    )).await?;
    let seen = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT 1 FROM \"_appstruct_billing_events\" WHERE event_id = $1",
        [event_id.into()],
    )).await?.is_some();
    if seen { transaction.commit().await?; return Ok(()); }
    let remote = stripe_subscription(config, subscription_id).await?;
    let customer = remote.get("customer").and_then(Value::as_str).ok_or(ApiError::Internal)?;
    let organization_id = remote.pointer("/metadata/appstruct_organization_id")
        .and_then(Value::as_str).and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(ApiError::Internal)?;
    let plan_id = remote.pointer("/metadata/appstruct_plan_id").and_then(Value::as_str)
        .filter(|value| PLANS.iter().any(|plan| plan.id == *value)).ok_or(ApiError::Internal)?;
    let linked = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT 1 FROM \"_appstruct_billing_customers\" WHERE organization_id = $1 AND provider_customer_id = $2",
        [organization_id.into(), customer.into()],
    )).await?.is_some();
    if !linked { return Err(ApiError::Internal); }
    let status = remote.get("status").and_then(Value::as_str).ok_or(ApiError::Internal)?;
    let period_end = remote.get("current_period_end").and_then(Value::as_i64)
        .or_else(|| remote.pointer("/items/data/0/current_period_end").and_then(Value::as_i64))
        .and_then(|seconds| chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0));
    let cancel_at_period_end = remote.get("cancel_at_period_end").and_then(Value::as_bool).unwrap_or(false);
    let event_created = event.get("created").and_then(Value::as_i64).ok_or(ApiError::Internal)?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_billing_subscriptions\" (id, organization_id, provider_subscription_id, plan_id, status, current_period_end, cancel_at_period_end, provider_event_created_at, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (provider_subscription_id) DO UPDATE SET plan_id = EXCLUDED.plan_id, status = EXCLUDED.status, current_period_end = EXCLUDED.current_period_end, cancel_at_period_end = EXCLUDED.cancel_at_period_end, provider_event_created_at = EXCLUDED.provider_event_created_at, updated_at = CURRENT_TIMESTAMP WHERE \"_appstruct_billing_subscriptions\".provider_event_created_at <= EXCLUDED.provider_event_created_at",
        [uuid::Uuid::now_v7().into(), organization_id.into(), subscription_id.into(), plan_id.into(), status.into(), period_end.into(), cancel_at_period_end.into(), event_created.into()],
    )).await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_billing_events\" (event_id, event_type, payload, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
        [event_id.into(), event_type.into(), json!(event).into()],
    )).await?;
    transaction.commit().await?;
    Ok(())
}
