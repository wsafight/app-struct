use super::config;
use super::handlers::{normalize_email, quote_ident};
use super::session::{cookie_value, random_token};
use crate::{ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::routing::{delete, get};
use axum::Router;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};

const ENABLED_PROVIDERS: &[&str] = &[];
const SIGNUP_DISABLED_PROVIDERS: &[&str] = &[];

pub(super) fn router() -> Router<AppState> {
    let mut router = Router::new();
    if !ENABLED_PROVIDERS.is_empty() {
        router = router
            .route("/api/auth/accounts", get(list_accounts))
            .route("/api/auth/accounts/{provider}", delete(unlink_account));
    }
    if enabled("oidc") {
        router = router
            .route("/api/auth/oauth/oidc/start", get(start_oidc))
            .route("/api/auth/oauth/oidc/callback", get(callback_oidc))
            .route("/api/auth/oauth/oidc/link/start", get(start_oidc_link))
            .route("/api/auth/oauth/oidc/link/callback", get(callback_oidc_link));
    }
    if enabled("google") {
        router = router
            .route("/api/auth/oauth/google/start", get(start_google))
            .route("/api/auth/oauth/google/callback", get(callback_google))
            .route("/api/auth/oauth/google/link/start", get(start_google_link))
            .route("/api/auth/oauth/google/link/callback", get(callback_google_link));
    }
    if enabled("github") {
        router = router
            .route("/api/auth/oauth/github/start", get(start_github))
            .route("/api/auth/oauth/github/callback", get(callback_github))
            .route("/api/auth/oauth/github/link/start", get(start_github_link))
            .route("/api/auth/oauth/github/link/callback", get(callback_github_link));
    }
    router
}

#[derive(Deserialize)]
struct OAuthCallback {
    code: String,
    state: String,
}

async fn start_oidc(State(state): State<AppState>) -> Result<(HeaderMap, Redirect), ApiError> {
    start_oauth("oidc", None, state.auth.config.secure_cookie).await
}

async fn start_google(State(state): State<AppState>) -> Result<(HeaderMap, Redirect), ApiError> {
    start_oauth("google", None, state.auth.config.secure_cookie).await
}

async fn start_github(State(state): State<AppState>) -> Result<(HeaderMap, Redirect), ApiError> {
    start_oauth("github", None, state.auth.config.secure_cookie).await
}

async fn start_oauth(
    provider: &str,
    link_user: Option<uuid::Uuid>,
    secure_cookie: bool,
) -> Result<(HeaderMap, Redirect), ApiError> {
    let config = provider_config(provider, link_user.is_some())?;
    let oauth_state = random_token();
    let secure = if secure_cookie { "; Secure" } else { "" };
    let scope = if provider == "github" {
        "read:user user:email"
    } else {
        "openid email profile"
    };
    let url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
        config.authorization_url,
        query_escape(&config.client_id),
        query_escape(&config.redirect_uri),
        query_escape(scope),
        query_escape(&oauth_state),
    );
    let mut headers = HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        format!(
            "appstruct_oauth_state={oauth_state}; Path=/api/auth/oauth/{provider}; HttpOnly; SameSite=Lax{secure}; Max-Age=600"
        )
        .parse()
        .map_err(|_| ApiError::Internal)?,
    );
    if let Some(user_id) = link_user {
        headers.append(
            axum::http::header::SET_COOKIE,
            format!("appstruct_oauth_link_user={user_id}; Path=/api/auth/oauth/{provider}/link; HttpOnly; SameSite=Lax{secure}; Max-Age=600")
                .parse().map_err(|_| ApiError::Internal)?,
        );
    }
    Ok((headers, Redirect::temporary(&url)))
}

async fn callback_oidc(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: axum::extract::Query<OAuthCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    oauth_callback("oidc", state, headers, query, false).await
}

async fn callback_google(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: axum::extract::Query<OAuthCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    oauth_callback("google", state, headers, query, false).await
}

async fn callback_github(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: axum::extract::Query<OAuthCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    oauth_callback("github", state, headers, query, false).await
}

async fn start_oidc_link(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<(HeaderMap, Redirect), ApiError> {
    start_link("oidc", &state, &headers).await
}

async fn start_google_link(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<(HeaderMap, Redirect), ApiError> {
    start_link("google", &state, &headers).await
}

async fn start_github_link(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<(HeaderMap, Redirect), ApiError> {
    start_link("github", &state, &headers).await
}

async fn start_link(
    provider: &str,
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(HeaderMap, Redirect), ApiError> {
    let actor = state
        .auth
        .actor(&state.database, headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    start_oauth(provider, Some(actor.id), state.auth.config.secure_cookie).await
}

async fn callback_oidc_link(
    State(state): State<AppState>, headers: HeaderMap,
    query: axum::extract::Query<OAuthCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    oauth_callback("oidc", state, headers, query, true).await
}

async fn callback_google_link(
    State(state): State<AppState>, headers: HeaderMap,
    query: axum::extract::Query<OAuthCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    oauth_callback("google", state, headers, query, true).await
}

async fn callback_github_link(
    State(state): State<AppState>, headers: HeaderMap,
    query: axum::extract::Query<OAuthCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    oauth_callback("github", state, headers, query, true).await
}

#[derive(Serialize)]
struct LinkedAccount {
    provider: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_accounts(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<axum::Json<Vec<LinkedAccount>>, ApiError> {
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let rows = state
        .database
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT provider, created_at FROM \"_appstruct_auth_oauth_accounts\" WHERE user_id = $1 ORDER BY created_at, provider",
            [actor.id.into()],
        ))
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LinkedAccount {
                provider: row.try_get("", "provider")?,
                created_at: row.try_get("", "created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sea_orm::DbErr>>()
        .map(axum::Json)
        .map_err(ApiError::from)
}

async fn unlink_account(
    State(state): State<AppState>, headers: HeaderMap, Path(provider): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .auth
        .actor_for_mutation(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !enabled(&provider) {
        return Err(ApiError::NotFound);
    }
    let transaction = state.database.begin().await?;
    let account = transaction
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT (SELECT COUNT(*) FROM \"_appstruct_auth_oauth_accounts\" WHERE user_id = $1) AS oauth_count, a.password_hash FROM \"_appstruct_auth_accounts\" a WHERE a.user_id = $1",
            [actor.id.into()],
        ))
        .await?
        .ok_or(ApiError::Internal)?;
    let count: i64 = account.try_get("", "oauth_count")?;
    let password_hash: String = account.try_get("", "password_hash")?;
    if count <= 1 && password_hash.is_empty() {
        return Err(ApiError::Conflict(
            "The last linked login method cannot be removed".to_owned(),
        ));
    }
    let deleted = transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM \"_appstruct_auth_oauth_accounts\" WHERE user_id = $1 AND provider = $2",
            [actor.id.into(), provider.clone().into()],
        ))
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    transaction.commit().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn link_oauth_account(
    state: &AppState,
    user_id: uuid::Uuid,
    provider: &str,
    subject: &str,
) -> Result<(), ApiError> {
    if state
        .database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT user_id FROM \"_appstruct_auth_oauth_accounts\" WHERE provider = $1 AND subject = $2",
            [provider.to_owned().into(), subject.to_owned().into()],
        ))
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict(
            "This provider account is already linked".to_owned(),
        ));
    }
    state
        .database
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_oauth_accounts\" (provider, subject, user_id, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
            [provider.to_owned().into(), subject.to_owned().into(), user_id.into()],
        ))
        .await?;
    Ok(())
}

fn clear_link_cookies(provider: &str, secure_cookie: bool) -> Result<HeaderMap, ApiError> {
    let secure = if secure_cookie { "; Secure" } else { "" };
    let mut headers = HeaderMap::new();
    for cookie in [
        format!("appstruct_oauth_state=; Path=/api/auth/oauth/{provider}; Max-Age=0; HttpOnly; SameSite=Lax{secure}"),
        format!("appstruct_oauth_link_user=; Path=/api/auth/oauth/{provider}/link; Max-Age=0; HttpOnly; SameSite=Lax{secure}"),
    ] {
        headers.append(
            axum::http::header::SET_COOKIE,
            cookie.parse().map_err(|_| ApiError::Internal)?,
        );
    }
    Ok(headers)
}

async fn oauth_callback(
    provider: &str,
    state: AppState,
    headers: HeaderMap,
    axum::extract::Query(input): axum::extract::Query<OAuthCallback>,
    linking: bool,
) -> Result<(HeaderMap, Redirect), ApiError> {
    let expected = cookie_value(&headers, "appstruct_oauth_state")
        .ok_or(ApiError::InvalidOAuthState)?;
    if expected != input.state {
        return Err(ApiError::InvalidOAuthState);
    }
    let provider_config = provider_config(provider, linking)?;
    let token_response = reqwest::Client::new()
        .post(&provider_config.token_url)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", input.code.as_str()),
            ("redirect_uri", provider_config.redirect_uri.as_str()),
            ("client_id", provider_config.client_id.as_str()),
            ("client_secret", provider_config.client_secret.as_str()),
        ])
        .send()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    if !token_response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let token_body: serde_json::Value = token_response
        .json()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    let access_token = token_body
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::OAuthProvider)?;
    let userinfo_response = reqwest::Client::new()
        .get(&provider_config.userinfo_url)
        .header("User-Agent", "appstruct-generated-app")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    if !userinfo_response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let claims: serde_json::Value = userinfo_response
        .json()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    let subject = claims
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            claims
                .get("id")
                .and_then(serde_json::Value::as_i64)
                .map(|id| id.to_string())
        })
        .ok_or(ApiError::OAuthProvider)?;
    let (email, verified) = if provider == "github" {
        github_email(access_token).await?
    } else {
        (
            claims
                .get("email")
                .and_then(serde_json::Value::as_str)
                .ok_or(ApiError::OAuthProvider)?
                .to_owned(),
            claims.get("email_verified").and_then(serde_json::Value::as_bool) == Some(true),
        )
    };
    if !verified {
        return Err(ApiError::OAuthProvider);
    }
    let email = normalize_email(&email)?;
    if linking {
        let actor = state
            .auth
            .actor(&state.database, &headers)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        let linked_user = cookie_value(&headers, "appstruct_oauth_link_user")
            .and_then(|value| uuid::Uuid::parse_str(&value).ok())
            .ok_or(ApiError::InvalidOAuthState)?;
        if actor.id != linked_user {
            return Err(ApiError::InvalidOAuthState);
        }
        link_oauth_account(&state, actor.id, provider, &subject).await?;
        return Ok((
            clear_link_cookies(provider, state.auth.config.secure_cookie)?,
            Redirect::temporary("/account"),
        ));
    }
    let user_id = find_or_create_oauth_user(&state, provider, &subject, &email).await?;
    let (session, csrf) = state.auth.create_session(&state.database, user_id).await?;
    let mut response_headers = state.auth.session_headers(&session, &csrf);
    let secure = if state.auth.config.secure_cookie { "; Secure" } else { "" };
    response_headers.append(
        axum::http::header::SET_COOKIE,
        format!(
            "appstruct_oauth_state=; Path=/api/auth/oauth/{provider}; Max-Age=0; HttpOnly; SameSite=Lax{secure}"
        )
        .parse()
        .map_err(|_| ApiError::Internal)?,
    );
    Ok((response_headers, Redirect::temporary("/")))
}

async fn github_email(access_token: &str) -> Result<(String, bool), ApiError> {
    let response = reqwest::Client::new()
        .get("https://api.github.com/user/emails")
        .header("User-Agent", "appstruct-generated-app")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    if !response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let emails: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    emails
        .iter()
        .find(|email| {
            email.get("primary").and_then(serde_json::Value::as_bool) == Some(true)
                && email.get("verified").and_then(serde_json::Value::as_bool) == Some(true)
        })
        .and_then(|email| email.get("email").and_then(serde_json::Value::as_str))
        .map(|email| (email.to_owned(), true))
        .ok_or(ApiError::OAuthProvider)
}

async fn find_or_create_oauth_user(
    state: &AppState,
    provider: &str,
    subject: &str,
    email: &str,
) -> Result<uuid::Uuid, ApiError> {
    if let Some(row) = state
        .database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT user_id FROM \"_appstruct_auth_oauth_accounts\" WHERE provider = $1 AND subject = $2",
            [provider.to_owned().into(), subject.to_owned().into()],
        ))
        .await?
    {
        return Ok(row.try_get("", "user_id")?);
    }
    let transaction = state.database.begin().await?;
    let user_id = if let Some(row) = transaction
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "SELECT {id} FROM {users} WHERE LOWER({email}) = $1",
                id = quote_ident(config::USER_ID_COLUMN),
                users = quote_ident(config::USER_TABLE),
                email = quote_ident(config::USER_EMAIL_COLUMN),
            ),
            [email.to_owned().into()],
        ))
        .await?
    {
        row.try_get("", config::USER_ID_COLUMN)?
    } else {
        if !signup_allowed(provider) {
            return Err(ApiError::Forbidden);
        }
        let id = uuid::Uuid::now_v7();
        transaction
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "INSERT INTO {users} ({id}, {email}) VALUES ($1, $2)",
                    users = quote_ident(config::USER_TABLE),
                    id = quote_ident(config::USER_ID_COLUMN),
                    email = quote_ident(config::USER_EMAIL_COLUMN),
                ),
                [id.into(), email.to_owned().into()],
            ))
            .await?;
        transaction
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_auth_accounts\" (user_id, password_hash, roles, email_verified_at, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                [id.into(), "".to_owned().into(), serde_json::json!([config::DEFAULT_ROLE]).into()],
            ))
            .await?;
        id
    };
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_oauth_accounts\" (provider, subject, user_id, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
            [provider.to_owned().into(), subject.to_owned().into(), user_id.into()],
        ))
        .await?;
    transaction.commit().await?;
    Ok(user_id)
}

struct ProviderConfig {
    authorization_url: String,
    token_url: String,
    userinfo_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

fn provider_config(provider: &str, linking: bool) -> Result<ProviderConfig, ApiError> {
    if !enabled(provider) {
        return Err(ApiError::OAuthConfiguration);
    }
    let (authorization_url, token_url, userinfo_url, prefix) = match provider {
        "oidc" => (
            required_env("APPSTRUCT_OIDC_AUTHORIZATION_URL")?,
            required_env("APPSTRUCT_OIDC_TOKEN_URL")?,
            required_env("APPSTRUCT_OIDC_USERINFO_URL")?,
            "APPSTRUCT_OIDC",
        ),
        "google" => (
            "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
            "https://oauth2.googleapis.com/token".to_owned(),
            "https://openidconnect.googleapis.com/v1/userinfo".to_owned(),
            "APPSTRUCT_GOOGLE",
        ),
        "github" => (
            "https://github.com/login/oauth/authorize".to_owned(),
            "https://github.com/login/oauth/access_token".to_owned(),
            "https://api.github.com/user".to_owned(),
            "APPSTRUCT_GITHUB",
        ),
        _ => return Err(ApiError::OAuthConfiguration),
    };
    let redirect_uri = required_env(&format!("{prefix}_REDIRECT_URI"))?;
    let redirect_uri = if linking {
        redirect_uri
            .strip_suffix("/callback")
            .map(|base| format!("{base}/link/callback"))
            .ok_or(ApiError::OAuthConfiguration)?
    } else {
        redirect_uri
    };
    Ok(ProviderConfig {
        authorization_url,
        token_url,
        userinfo_url,
        client_id: required_env(&format!("{prefix}_CLIENT_ID"))?,
        client_secret: required_env(&format!("{prefix}_CLIENT_SECRET"))?,
        redirect_uri,
    })
}

fn enabled(provider: &str) -> bool {
    ENABLED_PROVIDERS.contains(&provider)
}

fn signup_allowed(provider: &str) -> bool {
    !SIGNUP_DISABLED_PROVIDERS.contains(&provider)
}

fn required_env(name: &str) -> Result<String, ApiError> {
    std::env::var(name).map_err(|_| ApiError::OAuthConfiguration)
}

fn query_escape(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                vec![byte as char]
            } else {
                format!("%{byte:02X}").chars().collect()
            }
        })
        .collect()
}
