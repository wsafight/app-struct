mod config;
mod handlers;
mod mail;
mod session;

pub use mail::{AuthMailSender, DevMailSender, SmtpMailSender};
pub use session::AuthState;

use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    handlers::router()
}
