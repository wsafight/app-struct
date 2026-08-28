mod config;
mod handlers;
mod mail;
mod session;
#[allow(unused_imports)]
pub(crate) use session::{random_token, token_hash};

pub use mail::{AuthMailSender, DevMailSender, SmtpMailSender};
pub use session::AuthState;

use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    handlers::router()
}
