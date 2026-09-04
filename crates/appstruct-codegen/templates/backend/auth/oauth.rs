use super::config;
use super::handlers::{hash_password, normalize_email, quote_ident};
use super::session::{cookie_value, random_token};
use crate::{ApiError, AppState};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::routing::get;
use axum::Router;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::Deserialize;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/oauth/oidc/start", get(start_oidc))
        .route("/api/auth/oauth/oidc/callback", get(oidc_callback))
}

#[derive(Deserialize)]
struct OidcCallback { code: String, state: String }

async fn start_oidc() -> Result<(HeaderMap, Redirect), ApiError> {
    let oauth_state = random_token();
    let redirect_uri = required_env("APPSTRUCT_OIDC_REDIRECT_URI")?;
    let authorization_url = required_env("APPSTRUCT_OIDC_AUTHORIZATION_URL")?;
    let client_id = required_env("APPSTRUCT_OIDC_CLIENT_ID")?;
    let url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid%20email&state={}",
        authorization_url,
        query_escape(&client_id),
        query_escape(&redirect_uri),
        query_escape(&oauth_state),
    );
    let mut headers = HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        format!("appstruct_oidc_state={oauth_state}; Path=/api/auth/oauth/oidc; HttpOnly; SameSite=Lax; Max-Age=600")
            .parse()
            .map_err(|_| ApiError::Internal)?,
    );
    Ok((headers, Redirect::temporary(&url)))
}

async fn oidc_callback(
    State(state): State<AppState>, headers: HeaderMap, Query(input): Query<OidcCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    let expected = cookie_value(&headers, "appstruct_oidc_state").ok_or(ApiError::InvalidOAuthState)?;
    if expected != input.state {
        return Err(ApiError::InvalidOAuthState);
    }
    let redirect_uri = required_env("APPSTRUCT_OIDC_REDIRECT_URI")?;
    let client_id = required_env("APPSTRUCT_OIDC_CLIENT_ID")?;
    let client_secret = required_env("APPSTRUCT_OIDC_CLIENT_SECRET")?;
    let token_url = required_env("APPSTRUCT_OIDC_TOKEN_URL")?;
    let token_response = reqwest::Client::new()
        .post(token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", input.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
        ])
        .send().await.map_err(|_| ApiError::OAuthProvider)?;
    if !token_response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let token_body: serde_json::Value = token_response.json().await.map_err(|_| ApiError::OAuthProvider)?;
    let access_token = token_body.get("access_token")
        .and_then(serde_json::Value::as_str).ok_or(ApiError::OAuthProvider)?;
    let userinfo_response = reqwest::Client::new()
        .get(required_env("APPSTRUCT_OIDC_USERINFO_URL")?)
        .bearer_auth(access_token)
        .send().await.map_err(|_| ApiError::OAuthProvider)?;
    if !userinfo_response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let claims: serde_json::Value = userinfo_response.json().await.map_err(|_| ApiError::OAuthProvider)?;
    let subject = claims.get("sub").and_then(serde_json::Value::as_str)
        .ok_or(ApiError::OAuthProvider)?;
    if claims.get("email_verified").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(ApiError::OAuthProvider);
    }
    let email = normalize_email(
        claims.get("email").and_then(serde_json::Value::as_str).ok_or(ApiError::OAuthProvider)?,
    )?;
    let user_id = find_or_create_oauth_user(&state, subject, &email).await?;
    let (session, csrf) = state.auth.create_session(&state.database, user_id).await?;
    let mut response_headers = state.auth.session_headers(&session, &csrf);
    response_headers.append(
        axum::http::header::SET_COOKIE,
        "appstruct_oidc_state=; Path=/api/auth/oauth/oidc; Max-Age=0; HttpOnly; SameSite=Lax"
            .parse().map_err(|_| ApiError::Internal)?,
    );
    Ok((response_headers, Redirect::temporary("/")))
}

async fn find_or_create_oauth_user(
    state: &AppState, subject: &str, email: &str,
) -> Result<uuid::Uuid, ApiError> {
    let provider_subject = format!("oidc:{subject}");
    if let Some(row) = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT user_id FROM \"_appstruct_auth_oauth_accounts\" WHERE provider = $1 AND subject = $2",
        ["oidc".to_owned().into(), provider_subject.clone().into()],
    )).await? {
        return Ok(row.try_get("", "user_id")?);
    }
    let transaction = state.database.begin().await?;
    let user_id = if let Some(row) = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT {id} FROM {users} WHERE LOWER({email}) = $1",
            id = quote_ident(config::USER_ID_COLUMN), users = quote_ident(config::USER_TABLE),
            email = quote_ident(config::USER_EMAIL_COLUMN),
        ),
        [email.to_owned().into()],
    )).await? {
        row.try_get("", config::USER_ID_COLUMN)?
    } else {
        let id = uuid::Uuid::now_v7();
        transaction.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "INSERT INTO {users} ({id}, {email}) VALUES ($1, $2)",
                users = quote_ident(config::USER_TABLE), id = quote_ident(config::USER_ID_COLUMN),
                email = quote_ident(config::USER_EMAIL_COLUMN),
            ),
            [id.into(), email.to_owned().into()],
        )).await?;
        transaction.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_accounts\" (user_id, password_hash, roles, email_verified_at, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            [id.into(), hash_password(&random_token())?.into(), serde_json::json!([config::DEFAULT_ROLE]).into()],
        )).await?;
        id
    };
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_auth_oauth_accounts\" (provider, subject, user_id, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
        ["oidc".to_owned().into(), provider_subject.into(), user_id.into()],
    )).await?;
    transaction.commit().await?;
    Ok(user_id)
}

fn required_env(name: &str) -> Result<String, ApiError> {
    std::env::var(name).map_err(|_| ApiError::OAuthConfiguration)
}

fn query_escape(value: &str) -> String {
    value.bytes().flat_map(|byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            vec![byte as char]
        } else {
            format!("%{byte:02X}").chars().collect()
        }
    }).collect()
}
