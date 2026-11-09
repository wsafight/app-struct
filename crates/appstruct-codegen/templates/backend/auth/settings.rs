use super::{config, handlers::{hash_password, normalize_email, quote_ident, validate_password, verify_password}, session::{random_token, token_hash}};
use crate::{ApiError, AppState};
use crate::database::{DatabaseExecutor, Sql, Transaction};
use axum::{Json, Router, extract::State, http::{HeaderMap, StatusCode}, routing::{get, post}};
use base64::Engine as _;
use ring::{aead, hmac, rand::{SecureRandom, SystemRandom}};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/auth/settings", get(profile).patch(update_profile))
        .route("/api/auth/settings/password", post(change_password))
        .route("/api/auth/settings/email", post(change_email))
        .route("/api/auth/settings/email/confirm", post(confirm_email))
        .route("/api/auth/settings/mfa/setup", post(setup_mfa))
        .route("/api/auth/settings/mfa/enable", post(enable_mfa))
        .route("/api/auth/settings/mfa/disable", post(disable_mfa))
        .route("/api/auth/settings/sessions/revoke", post(revoke_sessions))
}

async fn ensure_profile<C: DatabaseExecutor>(database: &C, user: uuid::Uuid) -> Result<(), ApiError> {
    database.execute(Sql::bind("INSERT INTO \"_appstruct_product_profiles\" (user_id, display_name, locale, timezone, notifications, mfa_counter, mfa_recovery_hashes) VALUES ($1, '', 'en-US', 'UTC', true, -1, '[]'::jsonb) ON CONFLICT (user_id) DO NOTHING", [user.into()])).await?;
    Ok(())
}

async fn profile(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = state.auth.actor(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    let row = state.database.fetch_optional(Sql::bind("SELECT display_name, locale, timezone, notifications, mfa_secret IS NOT NULL AS mfa_enabled FROM \"_appstruct_product_profiles\" WHERE user_id = $1", [user.id.into()])).await?;
    let value = if let Some(row) = row { json!({ "display_name": row.try_get::<String, _>("display_name")?, "locale": row.try_get::<String, _>("locale")?, "timezone": row.try_get::<String, _>("timezone")?, "notifications": row.try_get::<bool, _>("notifications")?, "mfa_enabled": row.try_get::<bool, _>("mfa_enabled")? }) } else { json!({ "display_name": "", "locale": "en-US", "timezone": "UTC", "notifications": true, "mfa_enabled": false }) };
    Ok(Json(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileInput { display_name: String, locale: String, timezone: String, notifications: bool }

async fn update_profile(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<ProfileInput>) -> Result<StatusCode, ApiError> {
    let user = state.auth.actor_for_mutation(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    if input.display_name.len() > 120 || input.display_name.chars().any(char::is_control) || !matches!(input.locale.as_str(), "en-US" | "zh-CN") || !matches!(input.timezone.as_str(), "UTC" | "Asia/Shanghai") { return Err(ApiError::InvalidQuery("Invalid profile settings".to_owned())); }
    let transaction = state.database.begin().await?;
    ensure_profile(&transaction, user.id).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET display_name = $2, locale = $3, timezone = $4, notifications = $5 WHERE user_id = $1", [user.id.into(), input.display_name.trim().into(), input.locale.into(), input.timezone.into(), input.notifications.into()])).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof { password: String, code: Option<String> }

async fn reauthenticate(state: &AppState, headers: &HeaderMap, proof: &Proof) -> Result<uuid::Uuid, ApiError> {
    let user = state.auth.actor_for_mutation(&state.database, headers).await?.ok_or(ApiError::Unauthorized)?;
    state.auth.consume_auth_rate_limit(&state.database, &format!("settings:{}", user.id)).await?;
    let row = state.database.fetch_optional(Sql::bind("SELECT password_hash FROM \"_appstruct_auth_accounts\" WHERE user_id = $1", [user.id.into()])).await?.ok_or(ApiError::Unauthorized)?;
    let stored = row.try_get::<String, _>("password_hash")?;
    if !verify_password(proof.password.clone(), Some(stored)).await? { return Err(ApiError::Unauthorized); }
    Ok(user.id)
}

fn mfa_key() -> Result<aead::LessSafeKey, ApiError> {
    let unavailable = || ApiError::Conflict("Authenticator setup is unavailable; the operator must configure APPSTRUCT_AUTH_MFA_KEY as a base64-encoded 32-byte key".to_owned());
    let encoded = std::env::var("APPSTRUCT_AUTH_MFA_KEY").map_err(|_| unavailable())?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| unavailable())?;
    let key = aead::UnboundKey::new(&aead::AES_256_GCM, &bytes).map_err(|_| unavailable())?;
    Ok(aead::LessSafeKey::new(key))
}

fn seal(user: uuid::Uuid, secret: &[u8]) -> Result<String, ApiError> {
    let mut nonce = [0_u8; 12];
    SystemRandom::new().fill(&mut nonce).map_err(|_| ApiError::Internal)?;
    let mut bytes = secret.to_vec();
    mfa_key()?.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(user.as_bytes().as_slice()), &mut bytes).map_err(|_| ApiError::Internal)?;
    let mut envelope = nonce.to_vec(); envelope.extend(bytes);
    Ok(base64::engine::general_purpose::STANDARD.encode(envelope))
}

fn unseal(user: uuid::Uuid, encoded: &str) -> Result<Vec<u8>, ApiError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| ApiError::Internal)?;
    if bytes.len() < 28 { return Err(ApiError::Internal); }
    let nonce: [u8; 12] = bytes[..12].try_into().map_err(|_| ApiError::Internal)?;
    let mut sealed = bytes[12..].to_vec();
    let key = mfa_key()?;
    let secret = key.open_in_place(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(user.as_bytes().as_slice()), &mut sealed).map_err(|_| ApiError::Internal)?;
    Ok(secret.to_vec())
}

fn totp(secret: &[u8], counter: u64) -> String {
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret), &counter.to_be_bytes());
    let bytes = tag.as_ref();
    let offset = usize::from(bytes[19] & 15);
    let number = u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("SHA1 contains 20 bytes")) & 0x7fff_ffff;
    format!("{:06}", number % 1_000_000)
}

pub(super) async fn verify_mfa(transaction: &Transaction, user: uuid::Uuid, code: Option<&str>) -> Result<(), ApiError> {
    let row = transaction.fetch_optional(Sql::bind("SELECT mfa_secret, mfa_counter, mfa_recovery_hashes FROM \"_appstruct_product_profiles\" WHERE user_id = $1 FOR UPDATE", [user.into()])).await?;
    let Some(row) = row else { return Ok(()); };
    let Some(secret) = row.try_get::<Option<String>, _>("mfa_secret")? else { return Ok(()); };
    let code = code.ok_or(ApiError::Unauthorized)?;
    let mut hashes: Vec<String> = serde_json::from_value(row.try_get("mfa_recovery_hashes")?).map_err(|_| ApiError::Internal)?;
    if let Some(index) = hashes.iter().position(|hash| *hash == token_hash(code)) {
        hashes.remove(index);
        transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET mfa_recovery_hashes = $2 WHERE user_id = $1", [user.into(), json!(hashes).into()])).await?;
        return Ok(());
    }
    let secret = unseal(user, &secret)?;
    let current = chrono::Utc::now().timestamp() / 30;
    let previous: i64 = row.try_get("mfa_counter")?;
    let counter = (current - 1..=current + 1).find(|counter| *counter > previous && totp(&secret, *counter as u64) == code).ok_or(ApiError::Unauthorized)?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET mfa_counter = $2 WHERE user_id = $1", [user.into(), counter.into()])).await?;
    Ok(())
}

async fn setup_mfa(State(state): State<AppState>, headers: HeaderMap, Json(proof): Json<Proof>) -> Result<Json<Value>, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let mut secret = [0_u8; 20]; SystemRandom::new().fill(&mut secret).map_err(|_| ApiError::Internal)?;
    let encrypted = seal(user, &secret)?;
    let transaction = state.database.begin().await?;
    ensure_profile(&transaction, user).await?;
    let updated = transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET mfa_pending = $2, mfa_pending_expires = CURRENT_TIMESTAMP + INTERVAL '10 minutes' WHERE user_id = $1 AND mfa_secret IS NULL", [user.into(), encrypted.into()])).await?;
    if updated.rows_affected() == 0 { return Err(ApiError::Conflict("MFA is already enabled".to_owned())); }
    transaction.commit().await?;
    let encoded = data_encoding::BASE32_NOPAD.encode(&secret);
    Ok(Json(json!({ "secret": encoded, "uri": format!("otpauth://totp/AppStruct:{user}?secret={encoded}&issuer=AppStruct&algorithm=SHA1&digits=6&period=30") })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Code { code: String }

async fn enable_mfa(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Code>) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let user = state.auth.actor_for_mutation(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    state.auth.consume_auth_rate_limit(&state.database, &format!("mfa:{}", user.id)).await?;
    let transaction = state.database.begin().await?;
    let row = transaction.fetch_optional(Sql::bind("SELECT mfa_pending FROM \"_appstruct_product_profiles\" WHERE user_id = $1 AND mfa_secret IS NULL AND mfa_pending_expires > CURRENT_TIMESTAMP FOR UPDATE", [user.id.into()])).await?.ok_or(ApiError::Unauthorized)?;
    let encrypted = row.try_get::<Option<String>, _>("mfa_pending")?.ok_or(ApiError::Unauthorized)?;
    let secret = unseal(user.id, &encrypted)?;
    let current = chrono::Utc::now().timestamp() / 30;
    let counter = (current - 1..=current + 1).find(|counter| totp(&secret, *counter as u64) == input.code).ok_or(ApiError::Unauthorized)?;
    let recovery = (0..8).map(|_| random_token()).collect::<Vec<_>>();
    let hashes = recovery.iter().map(|code| token_hash(code)).collect::<Vec<_>>();
    transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET mfa_secret = mfa_pending, mfa_pending = NULL, mfa_pending_expires = NULL, mfa_counter = $2, mfa_recovery_hashes = $3 WHERE user_id = $1", [user.id.into(), counter.into(), json!(hashes).into()])).await?;
    revoke(&transaction, user.id).await?;
    let (session, csrf) = state.auth.create_session(&transaction, user.id).await?;
    transaction.commit().await?;
    Ok((state.auth.session_headers(&session, &csrf), Json(json!({ "recovery_codes": recovery, "reauthenticate": false }))))
}

async fn disable_mfa(State(state): State<AppState>, headers: HeaderMap, Json(proof): Json<Proof>) -> Result<StatusCode, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, proof.code.as_deref()).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET mfa_secret = NULL, mfa_pending = NULL, mfa_recovery_hashes = '[]'::jsonb, mfa_counter = -1 WHERE user_id = $1", [user.into()])).await?;
    revoke(&transaction, user).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke<C: DatabaseExecutor>(database: &C, user: uuid::Uuid) -> Result<(), ApiError> {
    database.execute(Sql::bind("UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL", [user.into()])).await?;
    database.execute(Sql::bind("UPDATE \"_appstruct_auth_api_tokens\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL", [user.into()])).await?;
    Ok(())
}

async fn revoke_sessions(State(state): State<AppState>, headers: HeaderMap, Json(proof): Json<Proof>) -> Result<StatusCode, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, proof.code.as_deref()).await?;
    revoke(&transaction, user).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordInput { password: String, code: Option<String>, new_password: String }

async fn change_password(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<PasswordInput>) -> Result<StatusCode, ApiError> {
    validate_password(&input.new_password)?;
    let user = reauthenticate(&state, &headers, &Proof { password: input.password, code: input.code.clone() }).await?;
    let hash = hash_password(&input.new_password).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, input.code.as_deref()).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_auth_accounts\" SET password_hash = $2 WHERE user_id = $1", [user.into(), hash.into()])).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_auth_password_resets\" SET used_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND used_at IS NULL", [user.into()])).await?;
    revoke(&transaction, user).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailInput { password: String, code: Option<String>, email: String }

async fn change_email(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<EmailInput>) -> Result<StatusCode, ApiError> {
    let user = reauthenticate(&state, &headers, &Proof { password: input.password, code: input.code.clone() }).await?;
    let email = normalize_email(&input.email)?;
    let token = random_token();
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, input.code.as_deref()).await?;
    ensure_profile(&transaction, user).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET email_pending = $2, email_token_hash = $3, email_expires_at = CURRENT_TIMESTAMP + INTERVAL '30 minutes' WHERE user_id = $1", [user.into(), email.clone().into(), token_hash(&token).into()])).await?;
    transaction.commit().await?;
    state.auth.mail.send_email_verification(&state.database, &email, &format!("{}/account?email_token={token}", state.auth.config.frontend_url)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Token { token: String }

async fn confirm_email(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Token>) -> Result<StatusCode, ApiError> {
    let actor = state.auth.actor_for_mutation(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    let transaction = state.database.begin().await?;
    let row = transaction.fetch_optional(Sql::bind("SELECT email_pending FROM \"_appstruct_product_profiles\" WHERE user_id = $1 AND email_token_hash = $2 AND email_expires_at > CURRENT_TIMESTAMP FOR UPDATE", [actor.id.into(), token_hash(&input.token).into()])).await?.ok_or(ApiError::Unauthorized)?;
    let email = row.try_get::<Option<String>, _>("email_pending")?.ok_or(ApiError::Unauthorized)?;
    let query = format!("UPDATE {} SET {} = $2 WHERE {} = $1", quote_ident(config::USER_TABLE), quote_ident(config::USER_EMAIL_COLUMN), quote_ident(config::USER_ID_COLUMN));
    transaction.execute(Sql::bind(query, [actor.id.into(), email.into()])).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_auth_accounts\" SET email_verified_at = CURRENT_TIMESTAMP WHERE user_id = $1", [actor.id.into()])).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_product_profiles\" SET email_pending = NULL, email_token_hash = NULL, email_expires_at = NULL WHERE user_id = $1", [actor.id.into()])).await?;
    transaction.execute(Sql::bind("UPDATE \"_appstruct_auth_password_resets\" SET used_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND used_at IS NULL", [actor.id.into()])).await?;
    revoke(&transaction, actor.id).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::totp;
    #[test]
    fn rfc6238_sha1_vectors() {
        for (seconds, expected) in [(59, "287082"), (1111111109, "081804"), (1111111111, "050471"), (1234567890, "005924"), (2000000000, "279037"), (20000000000, "353130")] {
            assert_eq!(totp(b"12345678901234567890", seconds / 30), expected);
        }
    }
}
