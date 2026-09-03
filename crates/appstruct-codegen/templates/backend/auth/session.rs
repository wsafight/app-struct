use super::config;
use super::mail::{AuthMailSender, DevMailSender, DisabledMailSender, SmtpMailSender};
use crate::{Actor, ApiError};
use axum::http::{HeaderMap, HeaderValue, Method, header};
use base64::Engine;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use sha2::{Digest, Sha256};
use std::env;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

const SESSION_COOKIE: &str = "appstruct_session";
const CSRF_COOKIE: &str = "appstruct_csrf";

#[derive(Clone)]
pub struct AuthState {
    pub(crate) config: Arc<AuthConfig>,
    pub(crate) mail: Arc<dyn AuthMailSender>,
}

pub(crate) struct AuthConfig {
    pub allowed_origin: String,
    pub frontend_url: String,
    pub secure_cookie: bool,
    pub session_ttl_hours: i64,
}

impl AuthState {
    pub fn from_env() -> Result<Self, String> {
        let production = env::var("APPSTRUCT_ENV").as_deref() == Ok("production");
        let mail_mode = env::var("APPSTRUCT_AUTH_MAIL_MODE").unwrap_or_else(|_| match (
            production,
            config::PASSWORD_RESET_ENABLED,
        ) {
            (true, true) => "missing".to_owned(),
            (true, false) => "disabled".to_owned(),
            (false, _) => "capture".to_owned(),
        });
        let mail: Arc<dyn AuthMailSender> = match mail_mode.as_str() {
            "capture" if !production => Arc::new(DevMailSender),
            "smtp" => Arc::new(SmtpMailSender::from_env()?),
            "missing" if config::PASSWORD_RESET_ENABLED => {
                return Err("APPSTRUCT_AUTH_MAIL_MODE=smtp is required in production when password reset is enabled".to_owned());
            }
            "disabled" if !config::PASSWORD_RESET_ENABLED => Arc::new(DisabledMailSender),
            other => return Err(format!("unsupported APPSTRUCT_AUTH_MAIL_MODE `{other}`")),
        };
        let secure_cookie = env::var("APPSTRUCT_COOKIE_SECURE")
            .map(|value| value != "false")
            .unwrap_or(production);
        let session_ttl_hours = env::var("APPSTRUCT_SESSION_TTL_HOURS")
            .ok()
            .map(|value| value.parse::<i64>())
            .transpose()
            .map_err(|error| format!("invalid APPSTRUCT_SESSION_TTL_HOURS: {error}"))?
            .unwrap_or(24 * 30);
        if session_ttl_hours <= 0 {
            return Err("APPSTRUCT_SESSION_TTL_HOURS must be positive".to_owned());
        }
        Ok(Self {
            config: Arc::new(AuthConfig {
                allowed_origin: load_public_origin(
                    "APPSTRUCT_ALLOWED_ORIGIN",
                    production,
                    "http://127.0.0.1:5173",
                )?,
                frontend_url: load_public_origin(
                    "APPSTRUCT_FRONTEND_URL",
                    production,
                    "http://127.0.0.1:5173",
                )?,
                secure_cookie,
                session_ttl_hours,
            }),
            mail,
        })
    }

    pub fn with_mail_sender(mut self, sender: Arc<dyn AuthMailSender>) -> Self {
        self.mail = sender;
        self
    }

    pub async fn actor(
        &self,
        database: &DatabaseConnection,
        headers: &HeaderMap,
    ) -> Result<Option<Actor>, ApiError> {
        if let Some(token) = cookie_value(headers, SESSION_COOKIE) {
            return self.actor_from_session(database, &token, None).await;
        }
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if let Some(token) = bearer {
            return self.actor_from_api_token(database, token).await;
        }
        Ok(None)
    }

    pub async fn actor_for_mutation(
        &self,
        database: &DatabaseConnection,
        headers: &HeaderMap,
    ) -> Result<Option<Actor>, ApiError> {
        self.validate_origin(headers)?;
        let Some(session) = cookie_value(headers, SESSION_COOKIE) else {
            return self.actor(database, headers).await;
        };
        let csrf = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .ok_or(ApiError::InvalidCsrf)?;
        self.actor_from_session(database, &session, Some(token_hash(csrf)))
            .await?
            .ok_or(ApiError::InvalidCsrf)
            .map(Some)
    }

    async fn actor_from_session(
        &self,
        database: &DatabaseConnection,
        token: &str,
        csrf_hash: Option<String>,
    ) -> Result<Option<Actor>, ApiError> {
        let sql = format!(
            "SELECT a.user_id, u.{email} AS email, a.roles FROM \"_appstruct_auth_sessions\" s JOIN \"_appstruct_auth_accounts\" a ON a.user_id = s.user_id JOIN {users} u ON u.{id} = a.user_id WHERE s.token_hash = $1 AND ($2::text IS NULL OR s.csrf_hash = $2) AND s.revoked_at IS NULL AND s.expires_at > CURRENT_TIMESTAMP",
            email = quote_ident(config::USER_EMAIL_COLUMN),
            users = quote_ident(config::USER_TABLE),
            id = quote_ident(config::USER_ID_COLUMN),
        );
        let Some(row) = database
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [token_hash(token).into(), csrf_hash.into()],
            ))
            .await?
        else {
            return Ok(None);
        };
        let roles: serde_json::Value = row.try_get("", "roles")?;
        let roles = serde_json::from_value(roles).map_err(|_| ApiError::Internal)?;
        Ok(Some(Actor {
            id: row.try_get("", "user_id")?,
            email: row.try_get("", "email")?,
            roles,
        }))
    }

    async fn actor_from_api_token(
        &self,
        database: &DatabaseConnection,
        token: &str,
    ) -> Result<Option<Actor>, ApiError> {
        let sql = format!(
            "SELECT a.user_id, u.{email} AS email, a.roles, (t.last_used_at IS NULL OR t.last_used_at < CURRENT_TIMESTAMP - INTERVAL '5 minutes') AS should_touch FROM \"_appstruct_auth_api_tokens\" t JOIN \"_appstruct_auth_accounts\" a ON a.user_id = t.user_id JOIN {users} u ON u.{id} = a.user_id WHERE t.token_hash = $1 AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at > CURRENT_TIMESTAMP)",
            email = quote_ident(config::USER_EMAIL_COLUMN),
            users = quote_ident(config::USER_TABLE),
            id = quote_ident(config::USER_ID_COLUMN),
        );
        let hash = token_hash(token);
        let Some(row) = database
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [hash.clone().into()],
            ))
            .await?
        else {
            return Ok(None);
        };
        if row.try_get::<bool>("", "should_touch")? {
            database
                .execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "UPDATE \"_appstruct_auth_api_tokens\" SET last_used_at = CURRENT_TIMESTAMP WHERE token_hash = $1 AND (last_used_at IS NULL OR last_used_at < CURRENT_TIMESTAMP - INTERVAL '5 minutes')",
                    [hash.into()],
                ))
                .await?;
        }
        let roles: serde_json::Value = row.try_get("", "roles")?;
        Ok(Some(Actor {
            id: row.try_get("", "user_id")?,
            email: row.try_get("", "email")?,
            roles: serde_json::from_value(roles).map_err(|_| ApiError::Internal)?,
        }))
    }

    pub async fn verify_csrf(
        &self,
        database: &DatabaseConnection,
        headers: &HeaderMap,
    ) -> Result<(), ApiError> {
        self.validate_origin(headers)?;
        // Bearer tokens are not sent automatically by browsers, so CSRF protection is
        // only required for cookie-authenticated mutations.
        if cookie_value(headers, SESSION_COOKIE).is_none()
            && headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("Bearer "))
        {
            return Ok(());
        }
        let Some(session) = cookie_value(headers, SESSION_COOKIE) else { return Ok(()) };
        let csrf = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .ok_or(ApiError::InvalidCsrf)?;
        let valid = database
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT 1 FROM \"_appstruct_auth_sessions\" WHERE token_hash = $1 AND csrf_hash = $2 AND revoked_at IS NULL AND expires_at > CURRENT_TIMESTAMP",
                [token_hash(&session).into(), token_hash(csrf).into()],
            ))
            .await?
            .is_some();
        if valid { Ok(()) } else { Err(ApiError::InvalidCsrf) }
    }

    pub(crate) fn validate_origin(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        if headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|origin| origin != self.config.allowed_origin)
        {
            return Err(ApiError::InvalidCsrf);
        }
        Ok(())
    }

    pub(crate) async fn check_login_rate<C: ConnectionTrait>(
        &self,
        database: &C,
        key: &str,
    ) -> Result<(), ApiError> {
        cleanup_login_attempts(database).await?;
        let Some(row) = database
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT attempts FROM "_appstruct_auth_login_attempts"
                   WHERE "key" = $1
                     AND window_started_at > CURRENT_TIMESTAMP - INTERVAL '1 minute'"#,
                [key.to_owned().into()],
            ))
            .await?
        else {
            return Ok(());
        };
        let attempts: i32 = row.try_get("", "attempts")?;
        if attempts >= 10 {
            return Err(ApiError::TooManyRequests);
        }
        Ok(())
    }

    pub(crate) async fn record_login_failure<C: ConnectionTrait>(
        &self,
        database: &C,
        key: &str,
    ) -> Result<(), ApiError> {
        database
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"INSERT INTO "_appstruct_auth_login_attempts" ("key", window_started_at, attempts)
                   VALUES ($1, CURRENT_TIMESTAMP, 1)
                   ON CONFLICT (key) DO UPDATE SET
                     attempts = CASE
                       WHEN "_appstruct_auth_login_attempts".window_started_at <= CURRENT_TIMESTAMP - INTERVAL '1 minute'
                       THEN 1
                       ELSE "_appstruct_auth_login_attempts".attempts + 1
                     END,
                     window_started_at = CASE
                       WHEN "_appstruct_auth_login_attempts".window_started_at <= CURRENT_TIMESTAMP - INTERVAL '1 minute'
                       THEN CURRENT_TIMESTAMP
                       ELSE "_appstruct_auth_login_attempts".window_started_at
                     END"#,
                [key.to_owned().into()],
            ))
            .await?;
        Ok(())
    }

    pub(crate) async fn clear_login_attempts<C: ConnectionTrait>(
        &self,
        database: &C,
        key: &str,
    ) -> Result<(), ApiError> {
        database
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"DELETE FROM "_appstruct_auth_login_attempts" WHERE "key" = $1"#,
                [key.to_owned().into()],
            ))
            .await?;
        Ok(())
    }

    pub fn cors_layer(&self) -> CorsLayer {
        CorsLayer::new()
            .allow_origin(self.config.allowed_origin.parse::<HeaderValue>().expect("validated origin"))
            .allow_credentials(true)
            .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
            .allow_headers([
                header::CONTENT_TYPE,
                header::IF_MATCH,
                header::AUTHORIZATION,
                "x-csrf-token".parse().unwrap(),
                "x-appstruct-tenant".parse().unwrap(),
            ])
            .expose_headers([header::ETAG])
    }

    pub(crate) async fn create_session<C: ConnectionTrait>(
        &self,
        database: &C,
        user_id: uuid::Uuid,
    ) -> Result<(String, String), ApiError> {
        let session = random_token();
        let csrf = random_token();
        database
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_auth_sessions\" (token_hash, user_id, csrf_hash, expires_at, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP + ($4 * INTERVAL '1 hour'), CURRENT_TIMESTAMP)",
                [
                    token_hash(&session).into(),
                    user_id.into(),
                    token_hash(&csrf).into(),
                    self.config.session_ttl_hours.into(),
                ],
            ))
            .await?;
        Ok((session, csrf))
    }

    pub(crate) fn session_headers(&self, session: &str, csrf: &str) -> HeaderMap {
        let secure = if self.config.secure_cookie { "; Secure" } else { "" };
        let mut headers = HeaderMap::new();
        headers.append(
            header::SET_COOKIE,
            format!("{SESSION_COOKIE}={session}; Path=/; HttpOnly; SameSite=Lax{secure}")
                .parse()
                .unwrap(),
        );
        headers.append(
            header::SET_COOKIE,
            format!("{CSRF_COOKIE}={csrf}; Path=/; SameSite=Lax{secure}")
                .parse()
                .unwrap(),
        );
        headers
    }

    pub(crate) fn clear_session_headers(&self) -> HeaderMap {
        let secure = if self.config.secure_cookie { "; Secure" } else { "" };
        let mut headers = HeaderMap::new();
        for (name, http_only) in [(SESSION_COOKIE, "; HttpOnly"), (CSRF_COOKIE, "")] {
            headers.append(
                header::SET_COOKIE,
                format!("{name}=; Path=/; Max-Age=0{http_only}; SameSite=Lax{secure}")
                    .parse()
                    .unwrap(),
            );
        }
        headers
    }
}

pub(crate) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(candidate, value)| (candidate == name).then(|| value.to_owned()))
}

pub(crate) fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub(crate) fn random_token() -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn load_public_origin(name: &str, production: bool, fallback: &str) -> Result<String, String> {
    match env::var(name) {
        Ok(value) => appstruct_runtime::validate_browser_origin(name, &value),
        Err(_) if production => Err(format!("{name} is required when APPSTRUCT_ENV=production")),
        Err(_) => Ok(fallback.to_owned()),
    }
}

async fn cleanup_login_attempts<C: ConnectionTrait>(database: &C) -> Result<(), ApiError> {
    database
        .execute_raw(Statement::from_string(
            DbBackend::Postgres,
            r#"DELETE FROM "_appstruct_auth_login_attempts"
               WHERE window_started_at <= CURRENT_TIMESTAMP - INTERVAL '1 minute'"#,
        ))
        .await?;
    Ok(())
}
