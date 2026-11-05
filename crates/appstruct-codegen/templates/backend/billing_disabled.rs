use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> { Router::new() }
pub fn validate_env() -> Result<(), String> { Ok(()) }
pub async fn has_entitlement(_context: &crate::RequestContext<'_>, _entitlement: &str) -> Result<bool, crate::ApiError> { Ok(false) }
