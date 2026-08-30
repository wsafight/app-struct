use super::config;
use super::handlers::{hash_password, normalize_email, quote_ident, validate_password};
use super::session::{random_token, token_hash};
use crate::{ApiError, AppState};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::Deserialize;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/email/request", post(request_email_verification))
        .route("/api/auth/email/verify", post(verify_email))
        .route("/api/auth/password/request", post(request_password_reset))
        .route("/api/auth/password/reset", post(reset_password))
}

#[derive(Deserialize)]
struct ResetRequest { email: String }

#[derive(Deserialize)]
struct ResetInput { token: String, password: String }

#[derive(Deserialize)]
struct VerifyEmailInput { token: String }

async fn request_email_verification(
    State(state): State<AppState>, headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let actor = state.auth.actor(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    if account_email_verified(&state, actor.id).await? {
        return Ok(StatusCode::NO_CONTENT);
    }
    issue_email_verification(&state, actor.id, &actor.email).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn verify_email(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<VerifyEmailInput>,
) -> Result<StatusCode, ApiError> {
    state.auth.validate_origin(&headers)?;
    let transaction = state.database.begin().await?;
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT user_id FROM \"_appstruct_auth_email_verifications\" WHERE token_hash = $1 AND used_at IS NULL AND expires_at > CURRENT_TIMESTAMP FOR UPDATE",
        [token_hash(&input.token).into()],
    )).await?.ok_or(ApiError::InvalidEmailVerificationToken)?;
    let user_id: uuid::Uuid = row.try_get("", "user_id")?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_auth_accounts\" SET email_verified_at = CURRENT_TIMESTAMP WHERE user_id = $1",
        [user_id.into()],
    )).await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_auth_email_verifications\" SET used_at = CURRENT_TIMESTAMP WHERE token_hash = $1",
        [token_hash(&input.token).into()],
    )).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn request_password_reset(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<ResetRequest>,
) -> Result<StatusCode, ApiError> {
    if !config::PASSWORD_RESET_ENABLED {
        return Err(ApiError::NotFound);
    }
    state.auth.validate_origin(&headers)?;
    let email = normalize_email(&input.email)?;
    state.auth.check_login_rate(&state.database, &format!("reset:{email}")).await?;
    if let Some(user_id) = find_user_id(&state, &email).await? {
        let token = random_token();
        state.database.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_password_resets\" (token_hash, user_id, expires_at, created_at) VALUES ($1, $2, CURRENT_TIMESTAMP + INTERVAL '30 minutes', CURRENT_TIMESTAMP)",
            [token_hash(&token).into(), user_id.into()],
        )).await?;
        let url = format!("{}/reset-password?token={token}", state.auth.config.frontend_url);
        state.auth.mail.send_password_reset(&state.database, &email, &url).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_password(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<ResetInput>,
) -> Result<StatusCode, ApiError> {
    if !config::PASSWORD_RESET_ENABLED {
        return Err(ApiError::NotFound);
    }
    state.auth.validate_origin(&headers)?;
    validate_password(&input.password)?;
    let password_hash = hash_password(&input.password)?;
    let transaction = state.database.begin().await?;
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT user_id FROM \"_appstruct_auth_password_resets\" WHERE token_hash = $1 AND used_at IS NULL AND expires_at > CURRENT_TIMESTAMP FOR UPDATE",
        [token_hash(&input.token).into()],
    )).await?.ok_or(ApiError::InvalidResetToken)?;
    let user_id: uuid::Uuid = row.try_get("", "user_id")?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_auth_accounts\" SET password_hash = $1 WHERE user_id = $2",
        [password_hash.into(), user_id.into()],
    )).await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_auth_password_resets\" SET used_at = CURRENT_TIMESTAMP WHERE token_hash = $1",
        [token_hash(&input.token).into()],
    )).await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL",
        [user_id.into()],
    )).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn find_user_id(state: &AppState, email: &str) -> Result<Option<uuid::Uuid>, ApiError> {
    let sql = format!(
        "SELECT a.user_id FROM \"_appstruct_auth_accounts\" a JOIN {users} u ON u.{id} = a.user_id WHERE LOWER(u.{email}) = $1",
        users = quote_ident(config::USER_TABLE),
        id = quote_ident(config::USER_ID_COLUMN),
        email = quote_ident(config::USER_EMAIL_COLUMN),
    );
    state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres, sql, [email.to_owned().into()],
    )).await?.map(|row| row.try_get("", "user_id").map_err(ApiError::from)).transpose()
}

pub(super) async fn account_email_verified(
    state: &AppState, user_id: uuid::Uuid,
) -> Result<bool, ApiError> {
    Ok(state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT email_verified_at FROM \"_appstruct_auth_accounts\" WHERE user_id = $1",
        [user_id.into()],
    )).await?.and_then(|row| {
        row.try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "email_verified_at").ok().flatten()
    }).is_some())
}

pub(super) async fn issue_email_verification(
    state: &AppState, user_id: uuid::Uuid, email: &str,
) -> Result<(), ApiError> {
    let token = random_token();
    state.database.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "DELETE FROM \"_appstruct_auth_email_verifications\" WHERE user_id = $1 AND used_at IS NULL",
        [user_id.into()],
    )).await?;
    state.database.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_auth_email_verifications\" (token_hash, user_id, expires_at, created_at) VALUES ($1, $2, CURRENT_TIMESTAMP + INTERVAL '24 hours', CURRENT_TIMESTAMP)",
        [token_hash(&token).into(), user_id.into()],
    )).await?;
    let url = format!("{}/verify-email?token={token}", state.auth.config.frontend_url);
    if let Err(error) = state.auth.mail.send_email_verification(&state.database, email, &url).await {
        tracing::warn!(?error, "email verification delivery failed");
    }
    Ok(())
}
