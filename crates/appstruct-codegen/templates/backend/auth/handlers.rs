use super::config;
use super::session::{cookie_value, random_token, token_hash};
use crate::{Actor, ApiError, AppState};
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/auth/password/request", post(request_password_reset))
        .route("/api/auth/password/reset", post(reset_password))
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct ResetRequest {
    email: String,
}

#[derive(Deserialize)]
struct ResetInput {
    token: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    user: Actor,
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<(HeaderMap, Json<AuthResponse>), ApiError> {
    if !config::REGISTRATION_ENABLED {
        return Err(ApiError::NotFound);
    }
    state.auth.validate_origin(&headers)?;
    let email = validate_credentials(input)?;
    let password_hash = hash_password(&email.password)?;
    let user_id = uuid::Uuid::now_v7();
    let roles = vec![config::DEFAULT_ROLE.to_owned()];
    let transaction = state.database.begin().await?;
    let insert_user = format!(
        "INSERT INTO {table} ({id}, {email}) VALUES ($1, $2)",
        table = quote_ident(config::USER_TABLE),
        id = quote_ident(config::USER_ID_COLUMN),
        email = quote_ident(config::USER_EMAIL_COLUMN),
    );
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            insert_user,
            [user_id.into(), email.email.clone().into()],
        ))
        .await?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_accounts\" (user_id, password_hash, roles, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
            [
                user_id.into(),
                password_hash.into(),
                serde_json::json!(roles).into(),
            ],
        ))
        .await?;
    let (session, csrf) = state.auth.create_session(&transaction, user_id).await?;
    transaction.commit().await?;
    let user = Actor {
        id: user_id,
        email: email.email,
        roles,
    };
    Ok((state.auth.session_headers(&session, &csrf), Json(AuthResponse { user })))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<(HeaderMap, Json<AuthResponse>), ApiError> {
    state.auth.validate_origin(&headers)?;
    let email = normalize_email(&input.email)?;
    validate_password(&input.password)?;
    state.auth.check_login_rate(&email)?;
    let sql = format!(
        "SELECT a.user_id, a.password_hash, a.roles FROM \"_appstruct_auth_accounts\" a JOIN {users} u ON u.{id} = a.user_id WHERE LOWER(u.{email}) = $1",
        users = quote_ident(config::USER_TABLE),
        id = quote_ident(config::USER_ID_COLUMN),
        email = quote_ident(config::USER_EMAIL_COLUMN),
    );
    let row = state
        .database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [email.clone().into()],
        ))
        .await?;
    let stored = row
        .as_ref()
        .and_then(|value| value.try_get::<String>("", "password_hash").ok())
        .unwrap_or_else(dummy_password_hash);
    let valid = PasswordHash::new(&stored)
        .ok()
        .is_some_and(|hash| Argon2::default().verify_password(input.password.as_bytes(), &hash).is_ok());
    let Some(row) = row.filter(|_| valid) else { return Err(ApiError::Unauthorized) };
    let user_id = row.try_get("", "user_id")?;
    let roles: serde_json::Value = row.try_get("", "roles")?;
    let roles = serde_json::from_value(roles).map_err(|_| ApiError::Internal)?;
    let user = Actor { id: user_id, email, roles };
    let (session, csrf) = state.auth.create_session(&state.database, user_id).await?;
    Ok((state.auth.session_headers(&session, &csrf), Json(AuthResponse { user })))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, StatusCode), ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    if let Some(token) = cookie_value(&headers, "appstruct_session") {
        state.database
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE token_hash = $1",
                [token_hash(&token).into()],
            ))
            .await?;
    }
    Ok((state.auth.clear_session_headers(), StatusCode::NO_CONTENT))
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthResponse>, ApiError> {
    let context = state.context(&headers).await?;
    let user = context.actor().cloned().ok_or(ApiError::Unauthorized)?;
    Ok(Json(AuthResponse { user }))
}

async fn request_password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ResetRequest>,
) -> Result<StatusCode, ApiError> {
    if !config::PASSWORD_RESET_ENABLED {
        return Err(ApiError::NotFound);
    }
    state.auth.validate_origin(&headers)?;
    let email = normalize_email(&input.email)?;
    state.auth.check_login_rate(&format!("reset:{email}"))?;
    if let Some(user_id) = find_user_id(&state, &email).await? {
        let token = random_token();
        state.database
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_auth_password_resets\" (token_hash, user_id, expires_at, created_at) VALUES ($1, $2, CURRENT_TIMESTAMP + INTERVAL '30 minutes', CURRENT_TIMESTAMP)",
                [token_hash(&token).into(), user_id.into()],
            ))
            .await?;
        let url = format!("{}/reset-password?token={token}", state.auth.config.frontend_url);
        state.auth.mail.send_password_reset(&state.database, &email, &url).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ResetInput>,
) -> Result<StatusCode, ApiError> {
    if !config::PASSWORD_RESET_ENABLED {
        return Err(ApiError::NotFound);
    }
    state.auth.validate_origin(&headers)?;
    validate_password(&input.password)?;
    let password_hash = hash_password(&input.password)?;
    let transaction = state.database.begin().await?;
    let row = transaction
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT user_id FROM \"_appstruct_auth_password_resets\" WHERE token_hash = $1 AND used_at IS NULL AND expires_at > CURRENT_TIMESTAMP FOR UPDATE",
            [token_hash(&input.token).into()],
        ))
        .await?
        .ok_or(ApiError::InvalidResetToken)?;
    let user_id: uuid::Uuid = row.try_get("", "user_id")?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_accounts\" SET password_hash = $1 WHERE user_id = $2",
            [password_hash.into(), user_id.into()],
        ))
        .await?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_password_resets\" SET used_at = CURRENT_TIMESTAMP WHERE token_hash = $1",
            [token_hash(&input.token).into()],
        ))
        .await?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL",
            [user_id.into()],
        ))
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

struct ValidatedCredentials {
    email: String,
    password: String,
}

fn validate_credentials(input: Credentials) -> Result<ValidatedCredentials, ApiError> {
    validate_password(&input.password)?;
    Ok(ValidatedCredentials {
        email: normalize_email(&input.email)?,
        password: input.password,
    })
}

fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_ascii_lowercase();
    if email.len() > 320 || !email.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.')) {
        return Err(ApiError::InvalidCredentialsInput);
    }
    Ok(email)
}

fn validate_password(value: &str) -> Result<(), ApiError> {
    if (12..=1024).contains(&value.len()) { Ok(()) } else { Err(ApiError::InvalidCredentialsInput) }
}

fn hash_password(value: &str) -> Result<String, ApiError> {
    let salt = SaltString::encode_b64(&rand::random::<[u8; 16]>())
        .map_err(|_| ApiError::Internal)?;
    Argon2::default()
        .hash_password(value.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::Internal)
}

fn dummy_password_hash() -> String {
    hash_password("invalid-password-value").expect("static password is hashable")
}

async fn find_user_id(state: &AppState, email: &str) -> Result<Option<uuid::Uuid>, ApiError> {
    let sql = format!(
        "SELECT a.user_id FROM \"_appstruct_auth_accounts\" a JOIN {users} u ON u.{id} = a.user_id WHERE LOWER(u.{email}) = $1",
        users = quote_ident(config::USER_TABLE),
        id = quote_ident(config::USER_ID_COLUMN),
        email = quote_ident(config::USER_EMAIL_COLUMN),
    );
    state.database
        .query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, [email.to_owned().into()]))
        .await?
        .map(|row| row.try_get("", "user_id").map_err(ApiError::from))
        .transpose()
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
