use crate::AppState;
use axum::Router;

pub(super) fn router() -> Router<AppState> { Router::new() }
