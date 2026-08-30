use super::config;
use super::recovery::{account_email_verified, issue_email_verification};
use super::session::{cookie_value, random_token, token_hash};
use crate::{Actor, ApiError, AppState};
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{Path, State};
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
        .route("/api/auth/tokens", get(list_api_tokens).post(create_api_token))
        .route("/api/auth/tokens/{id}", axum::routing::delete(revoke_api_token))
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct CreateApiTokenInput {
    name: String,
    expires_in_days: Option<u32>,
}

#[derive(Serialize)]
struct ApiToken {
    id: uuid::Uuid,
    name: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct CreatedApiToken {
    #[serde(flatten)]
    metadata: ApiToken,
    token: String,
}

#[derive(Serialize)]
struct AuthResponse {
    user: Actor,
    email_verified: bool,
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
    issue_email_verification(&state, user_id, &email.email).await?;
    let user = Actor {
        id: user_id,
        email: email.email,
        roles,
    };
    Ok((state.auth.session_headers(&session, &csrf), Json(AuthResponse { user, email_verified: false })))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<(HeaderMap, Json<AuthResponse>), ApiError> {
    state.auth.validate_origin(&headers)?;
    let email = normalize_email(&input.email)?;
    validate_password(&input.password)?;
    state.auth.check_login_rate(&state.database, &email).await?;
    let sql = format!(
        "SELECT a.user_id, a.password_hash, a.roles, a.email_verified_at FROM \"_appstruct_auth_accounts\" a JOIN {users} u ON u.{id} = a.user_id WHERE LOWER(u.{email}) = $1",
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
    let email_verified = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "email_verified_at")
        .ok()
        .flatten()
        .is_some();
    let roles: serde_json::Value = row.try_get("", "roles")?;
    let roles = serde_json::from_value(roles).map_err(|_| ApiError::Internal)?;
    let user = Actor { id: user_id, email, roles };
    let (session, csrf) = state.auth.create_session(&state.database, user_id).await?;
    Ok((state.auth.session_headers(&session, &csrf), Json(AuthResponse { user, email_verified })))
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
    let user = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let email_verified = account_email_verified(&state, user.id).await?;
    Ok(Json(AuthResponse { user, email_verified }))
}

async fn list_api_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ApiToken>>, ApiError> {
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let rows = state
        .database
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id, name, created_at, last_used_at, expires_at, revoked_at FROM \"_appstruct_auth_api_tokens\" WHERE user_id = $1 ORDER BY created_at DESC, id DESC",
            [actor.id.into()],
        ))
        .await?;
    rows.into_iter()
        .map(api_token_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
        .map_err(ApiError::from)
}

async fn create_api_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateApiTokenInput>,
) -> Result<(StatusCode, Json<CreatedApiToken>), ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(ApiError::Validation(vec![crate::FieldViolation {
            field: "name".to_owned(),
            message: "Name must contain between 1 and 80 bytes".to_owned(),
        }]));
    }
    if input.expires_in_days.is_some_and(|days| !(1..=3650).contains(&days)) {
        return Err(ApiError::Validation(vec![crate::FieldViolation {
            field: "expires_in_days".to_owned(),
            message: "Expiration must be between 1 and 3650 days".to_owned(),
        }]));
    }
    let token = random_token();
    let id = uuid::Uuid::now_v7();
    let expires_at = input
        .expires_in_days
        .map(|days| chrono::Utc::now() + chrono::Duration::days(i64::from(days)));
    state
        .database
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_api_tokens\" (id, user_id, token_hash, name, expires_at, created_at) VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP)",
            [
                id.into(), actor.id.into(), token_hash(&token).into(), name.to_owned().into(),
                expires_at.into(),
            ],
        ))
        .await?;
    let metadata = ApiToken {
        id,
        name: name.to_owned(),
        created_at: chrono::Utc::now(),
        last_used_at: None,
        expires_at,
        revoked_at: None,
    };
    Ok((StatusCode::CREATED, Json(CreatedApiToken { metadata, token })))
}

async fn revoke_api_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    state
        .database
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_api_tokens\" SET revoked_at = CURRENT_TIMESTAMP WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
            [id.into(), actor.id.into()],
        ))
        .await?;
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

pub(super) fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_ascii_lowercase();
    if email.len() > 320 || !email.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.')) {
        return Err(ApiError::InvalidCredentialsInput);
    }
    Ok(email)
}

pub(super) fn validate_password(value: &str) -> Result<(), ApiError> {
    if (12..=1024).contains(&value.len()) { Ok(()) } else { Err(ApiError::InvalidCredentialsInput) }
}

pub(super) fn hash_password(value: &str) -> Result<String, ApiError> {
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

fn api_token_from_row(row: sea_orm::QueryResult) -> Result<ApiToken, sea_orm::DbErr> {
    Ok(ApiToken {
        id: row.try_get("", "id")?,
        name: row.try_get("", "name")?,
        created_at: row.try_get("", "created_at")?,
        last_used_at: row.try_get("", "last_used_at")?,
        expires_at: row.try_get("", "expires_at")?,
        revoked_at: row.try_get("", "revoked_at")?,
    })
}

pub(super) fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
