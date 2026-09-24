use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> { Router::new() }
pub fn validate_env() -> Result<(), String> { Ok(()) }
