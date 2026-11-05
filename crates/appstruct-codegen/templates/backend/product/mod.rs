mod credits;
mod inbox;
mod insights;
mod team;
mod payments;

use crate::database::{DatabaseExecutor, Sql};
use crate::{ApiError, AppState};
use axum::{Json, Router, extract::{Path, Query, State}, http::{HeaderMap, StatusCode}, routing::{get, post}};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;

pub use inbox::{notify, report_finished};
pub use team::check_capacity;
pub use insights::record_event;
pub use credits::{reserve_credits, refund_credits};
pub use payments::apply_billing_event;

pub fn router() -> Router<AppState> {
    inbox::router().merge(credits::router()).merge(insights::router()).merge(team::router())
}

async fn owner<C: DatabaseExecutor>(database: &C, tenant: uuid::Uuid, actor: uuid::Uuid) -> Result<(), ApiError> {
    if !__TENANT__ { return Err(ApiError::NotFound); }
    let row = database.fetch_optional(Sql::bind("SELECT role FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2", [tenant.into(), actor.into()])).await?.ok_or(ApiError::Forbidden)?;
    if row.try_get::<String, _>("role")? != "owner" { return Err(ApiError::Forbidden); }
    Ok(())
}

fn invalid(message: &str) -> ApiError { ApiError::InvalidQuery(message.to_owned()) }

fn setting(name: &str, fallback: i64, max: i64) -> Result<i64, ApiError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<i64>().ok().filter(|number| (0..=max).contains(number)).ok_or(ApiError::Internal),
        Err(std::env::VarError::NotPresent) => Ok(fallback),
        Err(_) => Err(ApiError::Internal),
    }
}

fn page(query: &Pagination) -> Result<(i64, i64), ApiError> {
    let page = query.page.unwrap_or(1);
    let size = query.page_size.unwrap_or(25);
    if !(1..=10000).contains(&page) || !(1..=100).contains(&size) { return Err(invalid("Invalid pagination")); }
    Ok((size, (page - 1) * size))
}

#[derive(Deserialize)]
struct Pagination { page: Option<i64>, page_size: Option<i64> }
