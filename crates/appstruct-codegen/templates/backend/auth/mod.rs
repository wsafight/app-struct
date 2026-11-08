mod admin;
mod admin_schedules;
mod admin_storage;
mod admin_webhooks;
mod config;
mod handlers;
mod mail;
mod oauth;
mod recovery;
mod saved_views;
mod session;
mod settings;
#[allow(unused_imports)]
pub(crate) use session::{random_token, token_hash};
#[allow(unused_imports)]
pub(crate) use recovery::{account_email_verified, find_user_id};

pub use mail::{AuthMailSender, DevMailSender, SmtpMailSender};
pub use session::AuthState;

use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    handlers::router()
        .merge(settings::router())
        .merge(recovery::router())
        .merge(oauth::router())
        .merge(admin::router())
        .merge(admin_schedules::router())
        .merge(admin_storage::router())
        .merge(admin_webhooks::router())
        .merge(saved_views::router())
}
