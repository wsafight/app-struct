use super::{
    config,
    handlers::{hash_password, normalize_email, quote_ident, validate_password, verify_password},
    session::{cookie_value, random_token, token_hash},
};
use crate::database::{DatabaseExecutor, Sql, Transaction};
use crate::{ApiError, AppState};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use base64::Engine as _;
use ring::{
    aead, hmac,
    rand::{SecureRandom, SystemRandom},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;

const OAUTH_MFA_AAD: &[u8] = b"appstruct:oauth-mfa:v1";

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/settings", get(profile).patch(update_profile))
        .route("/api/auth/settings/password", post(change_password))
        .route("/api/auth/settings/email", post(change_email))
        .route("/api/auth/settings/email/confirm", post(confirm_email))
        .route("/api/auth/settings/mfa/setup", post(setup_mfa))
        .route("/api/auth/settings/mfa/enable", post(enable_mfa))
        .route("/api/auth/settings/mfa/disable", post(disable_mfa))
        .route("/api/auth/settings/sessions/revoke", post(revoke_sessions))
        .route("/api/auth/settings/export", post(export_data))
        .route("/api/auth/settings/delete", post(delete_account))
}

async fn ensure_profile<C: DatabaseExecutor>(database: &C, user: uuid::Uuid) -> Result<(), ApiError> {
    database
        .execute(Sql::bind(
            "INSERT INTO \"_appstruct_product_profiles\" (user_id, display_name, locale, timezone, notifications, mfa_counter, mfa_recovery_hashes) VALUES ($1, '', 'en-US', 'UTC', true, -1, '[]'::jsonb) ON CONFLICT (user_id) DO NOTHING",
            [user.into()],
        ))
        .await?;
    Ok(())
}

async fn profile(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let account = state
        .database
        .fetch_optional(Sql::bind(
            "SELECT password_hash <> '' AS has_password FROM \"_appstruct_auth_accounts\" WHERE user_id = $1",
            [user.id.into()],
        ))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let has_password: bool = account.try_get("has_password")?;
    let row = state
        .database
        .fetch_optional(Sql::bind(
            "SELECT display_name, locale, timezone, notifications, mfa_secret IS NOT NULL AS mfa_enabled FROM \"_appstruct_product_profiles\" WHERE user_id = $1",
            [user.id.into()],
        ))
        .await?;
    let value = if let Some(row) = row {
        json!({
            "display_name": row.try_get::<String, _>("display_name")?,
            "locale": row.try_get::<String, _>("locale")?,
            "timezone": row.try_get::<String, _>("timezone")?,
            "notifications": row.try_get::<bool, _>("notifications")?,
            "mfa_enabled": row.try_get::<bool, _>("mfa_enabled")?,
            "has_password": has_password,
        })
    } else {
        json!({
            "display_name": "",
            "locale": "en-US",
            "timezone": "UTC",
            "notifications": true,
            "mfa_enabled": false,
            "has_password": has_password,
        })
    };
    Ok(Json(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileInput {
    display_name: String,
    locale: String,
    timezone: String,
    notifications: bool,
}

async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProfileInput>,
) -> Result<StatusCode, ApiError> {
    let user = state
        .auth
        .actor_for_mutation(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if input.display_name.len() > 120
        || input.display_name.chars().any(char::is_control)
        || !matches!(input.locale.as_str(), "en-US" | "zh-CN")
        || !matches!(input.timezone.as_str(), "UTC" | "Asia/Shanghai")
    {
        return Err(ApiError::InvalidQuery("Invalid profile settings".to_owned()));
    }
    let transaction = state.database.begin().await?;
    ensure_profile(&transaction, user.id).await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_product_profiles\" SET display_name = $2, locale = $3, timezone = $4, notifications = $5 WHERE user_id = $1",
            [
                user.id.into(),
                input.display_name.trim().into(),
                input.locale.into(),
                input.timezone.into(),
                input.notifications.into(),
            ],
        ))
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    #[serde(default)]
    password: String,
    code: Option<String>,
}

async fn reauthenticate(
    state: &AppState,
    headers: &HeaderMap,
    proof: &Proof,
) -> Result<uuid::Uuid, ApiError> {
    let user = state
        .auth
        .actor_for_mutation(&state.database, headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    state
        .auth
        .consume_auth_rate_limit(&state.database, &format!("settings:{}", user.id))
        .await?;
    let row = state
        .database
        .fetch_optional(Sql::bind(
            "SELECT password_hash FROM \"_appstruct_auth_accounts\" WHERE user_id = $1",
            [user.id.into()],
        ))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let stored = row.try_get::<String, _>("password_hash")?;
    if stored.is_empty() {
        // OAuth-only accounts have no local password yet; require a browser cookie session
        // (with Origin + CSRF already verified by actor_for_mutation) and reject unexpected passwords.
        if cookie_value(headers, "appstruct_session").is_none() || !proof.password.is_empty() {
            return Err(ApiError::Unauthorized);
        }
        return Ok(user.id);
    }
    if !verify_password(proof.password.clone(), Some(stored)).await? {
        return Err(ApiError::Unauthorized);
    }
    Ok(user.id)
}

fn decode_mfa_key(encoded: &str) -> Result<aead::LessSafeKey, ApiError> {
    let unavailable = || {
        ApiError::Conflict(
            "Authenticator setup is unavailable; the operator must configure APPSTRUCT_AUTH_MFA_KEY (and optional comma-separated APPSTRUCT_AUTH_MFA_PREVIOUS_KEYS) as base64-encoded 32-byte keys"
                .to_owned(),
        )
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| unavailable())?;
    let key = aead::UnboundKey::new(&aead::AES_256_GCM, &bytes).map_err(|_| unavailable())?;
    Ok(aead::LessSafeKey::new(key))
}

fn mfa_keys() -> Result<(aead::LessSafeKey, Vec<aead::LessSafeKey>), ApiError> {
    let primary_raw = std::env::var("APPSTRUCT_AUTH_MFA_KEY").map_err(|_| {
        ApiError::Conflict(
            "Authenticator setup is unavailable; the operator must configure APPSTRUCT_AUTH_MFA_KEY as a base64-encoded 32-byte key"
                .to_owned(),
        )
    })?;
    let primary = decode_mfa_key(&primary_raw)?;
    let mut previous = Vec::new();
    if let Ok(raw) = std::env::var("APPSTRUCT_AUTH_MFA_PREVIOUS_KEYS") {
        for part in raw.split(',').map(str::trim).filter(|part| !part.is_empty()) {
            previous.push(decode_mfa_key(part)?);
        }
    }
    Ok((primary, previous))
}

fn seal_bytes(aad: &[u8], plaintext: &[u8]) -> Result<String, ApiError> {
    let (primary, _) = mfa_keys()?;
    seal_with_key(&primary, aad, plaintext)
}

fn seal_with_key(
    key: &aead::LessSafeKey,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<String, ApiError> {
    let mut nonce = [0_u8; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| ApiError::Internal)?;
    let mut bytes = plaintext.to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(aad),
        &mut bytes,
    )
    .map_err(|_| ApiError::Internal)?;
    let mut envelope = nonce.to_vec();
    envelope.extend(bytes);
    Ok(base64::engine::general_purpose::STANDARD.encode(envelope))
}

fn unseal_bytes(aad: &[u8], encoded: &str) -> Result<(Vec<u8>, bool), ApiError> {
    let (primary, previous) = mfa_keys()?;
    unseal_with_keys(&primary, &previous, aad, encoded)
}

fn unseal_with_keys(
    primary: &aead::LessSafeKey,
    previous: &[aead::LessSafeKey],
    aad: &[u8],
    encoded: &str,
) -> Result<(Vec<u8>, bool), ApiError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| ApiError::Internal)?;
    if bytes.len() < 28 {
        return Err(ApiError::Internal);
    }
    let nonce: [u8; 12] = bytes[..12].try_into().map_err(|_| ApiError::Internal)?;
    let ciphertext = &bytes[12..];
    let mut primary_buf = ciphertext.to_vec();
    if let Ok(opened) = primary.open_in_place(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(aad),
        &mut primary_buf,
    ) {
        return Ok((opened.to_vec(), false));
    }
    for fallback in previous {
        let mut buf = ciphertext.to_vec();
        if let Ok(opened) = fallback.open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut buf,
        ) {
            return Ok((opened.to_vec(), true));
        }
    }
    Err(ApiError::Internal)
}

fn seal(user: uuid::Uuid, secret: &[u8]) -> Result<String, ApiError> {
    seal_bytes(user.as_bytes().as_slice(), secret)
}

fn unseal(user: uuid::Uuid, encoded: &str) -> Result<(Vec<u8>, bool), ApiError> {
    unseal_bytes(user.as_bytes().as_slice(), encoded)
}

#[allow(dead_code)]
pub(super) fn seal_oauth_mfa_challenge(user: uuid::Uuid) -> Result<String, ApiError> {
    let expires_at = chrono::Utc::now().timestamp() + 300;
    let mut payload = [0_u8; 24];
    payload[..16].copy_from_slice(user.as_bytes());
    payload[16..].copy_from_slice(&expires_at.to_be_bytes());
    seal_bytes(OAUTH_MFA_AAD, &payload)
}

#[allow(dead_code)]
pub(super) fn unseal_oauth_mfa_challenge(encoded: &str) -> Result<uuid::Uuid, ApiError> {
    let (payload, _) =
        unseal_bytes(OAUTH_MFA_AAD, encoded).map_err(|_| ApiError::InvalidOAuthState)?;
    if payload.len() != 24 {
        return Err(ApiError::InvalidOAuthState);
    }
    let user_bytes: [u8; 16] = payload[..16]
        .try_into()
        .map_err(|_| ApiError::InvalidOAuthState)?;
    let exp_bytes: [u8; 8] = payload[16..]
        .try_into()
        .map_err(|_| ApiError::InvalidOAuthState)?;
    let expires_at = i64::from_be_bytes(exp_bytes);
    if chrono::Utc::now().timestamp() > expires_at {
        return Err(ApiError::InvalidOAuthState);
    }
    Ok(uuid::Uuid::from_bytes(user_bytes))
}

fn totp(secret: &[u8], counter: u64) -> String {
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret),
        &counter.to_be_bytes(),
    );
    let bytes = tag.as_ref();
    let offset = usize::from(bytes[19] & 15);
    let number =
        u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("SHA1 contains 20 bytes"))
            & 0x7fff_ffff;
    format!("{:06}", number % 1_000_000)
}

pub(super) async fn verify_mfa(
    transaction: &Transaction,
    user: uuid::Uuid,
    code: Option<&str>,
) -> Result<(), ApiError> {
    let row = transaction
        .fetch_optional(Sql::bind(
            "SELECT mfa_secret, mfa_counter, mfa_recovery_hashes FROM \"_appstruct_product_profiles\" WHERE user_id = $1 FOR UPDATE",
            [user.into()],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let Some(secret_envelope) = row.try_get::<Option<String>, _>("mfa_secret")? else {
        return Ok(());
    };
    let code = code.ok_or(ApiError::Unauthorized)?;
    let mut hashes: Vec<String> =
        serde_json::from_value(row.try_get("mfa_recovery_hashes")?).map_err(|_| ApiError::Internal)?;
    if let Some(index) = hashes.iter().position(|hash| *hash == token_hash(code)) {
        hashes.remove(index);
        if let Ok((secret, true)) = unseal(user, &secret_envelope) {
            let rotated = seal(user, &secret)?;
            transaction
                .execute(Sql::bind(
                    "UPDATE \"_appstruct_product_profiles\" SET mfa_recovery_hashes = $2, mfa_secret = $3 WHERE user_id = $1",
                    [user.into(), json!(hashes).into(), rotated.into()],
                ))
                .await?;
        } else {
            transaction
                .execute(Sql::bind(
                    "UPDATE \"_appstruct_product_profiles\" SET mfa_recovery_hashes = $2 WHERE user_id = $1",
                    [user.into(), json!(hashes).into()],
                ))
                .await?;
        }
        return Ok(());
    }
    let (secret, rotated_needed) = unseal(user, &secret_envelope)?;
    let current = chrono::Utc::now().timestamp() / 30;
    let previous: i64 = row.try_get("mfa_counter")?;
    let counter = (current - 1..=current + 1)
        .find(|counter| *counter > previous && totp(&secret, *counter as u64) == code)
        .ok_or(ApiError::Unauthorized)?;
    if rotated_needed {
        let rotated = seal(user, &secret)?;
        transaction
            .execute(Sql::bind(
                "UPDATE \"_appstruct_product_profiles\" SET mfa_counter = $2, mfa_secret = $3 WHERE user_id = $1",
                [user.into(), counter.into(), rotated.into()],
            ))
            .await?;
    } else {
        transaction
            .execute(Sql::bind(
                "UPDATE \"_appstruct_product_profiles\" SET mfa_counter = $2 WHERE user_id = $1",
                [user.into(), counter.into()],
            ))
            .await?;
    }
    Ok(())
}

async fn setup_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<Proof>,
) -> Result<Json<Value>, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let mut secret = [0_u8; 20];
    SystemRandom::new()
        .fill(&mut secret)
        .map_err(|_| ApiError::Internal)?;
    let encrypted = seal(user, &secret)?;
    let transaction = state.database.begin().await?;
    ensure_profile(&transaction, user).await?;
    let updated = transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_product_profiles\" SET mfa_pending = $2, mfa_pending_expires = CURRENT_TIMESTAMP + INTERVAL '10 minutes' WHERE user_id = $1 AND mfa_secret IS NULL",
            [user.into(), encrypted.into()],
        ))
        .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::Conflict("MFA is already enabled".to_owned()));
    }
    transaction.commit().await?;
    let encoded = data_encoding::BASE32_NOPAD.encode(&secret);
    Ok(Json(json!({
        "secret": encoded,
        "uri": format!("otpauth://totp/AppStruct:{user}?secret={encoded}&issuer=AppStruct&algorithm=SHA1&digits=6&period=30"),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Code {
    code: String,
}

async fn enable_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Code>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let user = state
        .auth
        .actor_for_mutation(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    state
        .auth
        .consume_auth_rate_limit(&state.database, &format!("mfa:{}", user.id))
        .await?;
    let transaction = state.database.begin().await?;
    let row = transaction
        .fetch_optional(Sql::bind(
            "SELECT mfa_pending FROM \"_appstruct_product_profiles\" WHERE user_id = $1 AND mfa_secret IS NULL AND mfa_pending_expires > CURRENT_TIMESTAMP FOR UPDATE",
            [user.id.into()],
        ))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let encrypted = row
        .try_get::<Option<String>, _>("mfa_pending")?
        .ok_or(ApiError::Unauthorized)?;
    let (secret, rotated_needed) = unseal(user.id, &encrypted)?;
    let current = chrono::Utc::now().timestamp() / 30;
    let counter = (current - 1..=current + 1)
        .find(|counter| totp(&secret, *counter as u64) == input.code)
        .ok_or(ApiError::Unauthorized)?;
    let final_secret = if rotated_needed {
        seal(user.id, &secret)?
    } else {
        encrypted
    };
    let recovery = (0..8).map(|_| random_token()).collect::<Vec<_>>();
    let hashes = recovery.iter().map(|code| token_hash(code)).collect::<Vec<_>>();
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_product_profiles\" SET mfa_secret = $4, mfa_pending = NULL, mfa_pending_expires = NULL, mfa_counter = $2, mfa_recovery_hashes = $3 WHERE user_id = $1",
            [
                user.id.into(),
                counter.into(),
                json!(hashes).into(),
                final_secret.into(),
            ],
        ))
        .await?;
    revoke(&transaction, user.id).await?;
    let (session, csrf) = state.auth.create_session(&transaction, user.id).await?;
    transaction.commit().await?;
    Ok((
        state.auth.session_headers(&session, &csrf),
        Json(json!({ "recovery_codes": recovery, "reauthenticate": false })),
    ))
}

async fn disable_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<Proof>,
) -> Result<StatusCode, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, proof.code.as_deref()).await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_product_profiles\" SET mfa_secret = NULL, mfa_pending = NULL, mfa_recovery_hashes = '[]'::jsonb, mfa_counter = -1 WHERE user_id = $1",
            [user.into()],
        ))
        .await?;
    revoke(&transaction, user).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke<C: DatabaseExecutor>(database: &C, user: uuid::Uuid) -> Result<(), ApiError> {
    database
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL",
            [user.into()],
        ))
        .await?;
    database
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_api_tokens\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL",
            [user.into()],
        ))
        .await?;
    Ok(())
}

async fn revoke_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<Proof>,
) -> Result<StatusCode, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, proof.code.as_deref()).await?;
    revoke(&transaction, user).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordInput {
    #[serde(default)]
    password: String,
    code: Option<String>,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PasswordInput>,
) -> Result<StatusCode, ApiError> {
    validate_password(&input.new_password)?;
    let user = reauthenticate(
        &state,
        &headers,
        &Proof {
            password: input.password,
            code: input.code.clone(),
        },
    )
    .await?;
    let hash = hash_password(&input.new_password).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, input.code.as_deref()).await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_accounts\" SET password_hash = $2 WHERE user_id = $1",
            [user.into(), hash.into()],
        ))
        .await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_password_resets\" SET used_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND used_at IS NULL",
            [user.into()],
        ))
        .await?;
    revoke(&transaction, user).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailInput {
    #[serde(default)]
    password: String,
    code: Option<String>,
    email: String,
}

async fn change_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<EmailInput>,
) -> Result<StatusCode, ApiError> {
    let user = reauthenticate(
        &state,
        &headers,
        &Proof {
            password: input.password,
            code: input.code.clone(),
        },
    )
    .await?;
    let email = normalize_email(&input.email)?;
    let token = random_token();
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, input.code.as_deref()).await?;
    ensure_profile(&transaction, user).await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_product_profiles\" SET email_pending = $2, email_token_hash = $3, email_expires_at = CURRENT_TIMESTAMP + INTERVAL '30 minutes' WHERE user_id = $1",
            [user.into(), email.clone().into(), token_hash(&token).into()],
        ))
        .await?;
    transaction.commit().await?;
    state
        .auth
        .mail
        .send_email_verification(
            &state.database,
            &email,
            &format!("{}/account?email_token={token}", state.auth.config.frontend_url),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    token: String,
}

async fn confirm_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Token>,
) -> Result<StatusCode, ApiError> {
    let actor = state
        .auth
        .actor_for_mutation(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let transaction = state.database.begin().await?;
    let row = transaction
        .fetch_optional(Sql::bind(
            "SELECT email_pending FROM \"_appstruct_product_profiles\" WHERE user_id = $1 AND email_token_hash = $2 AND email_expires_at > CURRENT_TIMESTAMP FOR UPDATE",
            [actor.id.into(), token_hash(&input.token).into()],
        ))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let email = row
        .try_get::<Option<String>, _>("email_pending")?
        .ok_or(ApiError::Unauthorized)?;
    let query = format!(
        "UPDATE {} SET {} = $2 WHERE {} = $1",
        quote_ident(config::USER_TABLE),
        quote_ident(config::USER_EMAIL_COLUMN),
        quote_ident(config::USER_ID_COLUMN)
    );
    transaction
        .execute(Sql::bind(query, [actor.id.into(), email.into()]))
        .await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_accounts\" SET email_verified_at = CURRENT_TIMESTAMP WHERE user_id = $1",
            [actor.id.into()],
        ))
        .await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_product_profiles\" SET email_pending = NULL, email_token_hash = NULL, email_expires_at = NULL WHERE user_id = $1",
            [actor.id.into()],
        ))
        .await?;
    transaction
        .execute(Sql::bind(
            "UPDATE \"_appstruct_auth_password_resets\" SET used_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND used_at IS NULL",
            [actor.id.into()],
        ))
        .await?;
    revoke(&transaction, actor.id).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn export_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<Proof>,
) -> Result<Json<Value>, ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, proof.code.as_deref()).await?;
    let account_sql = format!(
        "SELECT u.{email} AS email, a.roles, a.email_verified_at, a.password_hash <> '' AS has_password, a.created_at FROM \"_appstruct_auth_accounts\" a JOIN {users} u ON u.{id} = a.user_id WHERE a.user_id = $1",
        email = quote_ident(config::USER_EMAIL_COLUMN),
        users = quote_ident(config::USER_TABLE),
        id = quote_ident(config::USER_ID_COLUMN),
    );
    let account = transaction
        .fetch_optional(Sql::bind(account_sql, [user.into()]))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let profile_row = transaction
        .fetch_optional(Sql::bind(
            "SELECT display_name, locale, timezone, notifications, mfa_secret IS NOT NULL AS mfa_enabled FROM \"_appstruct_product_profiles\" WHERE user_id = $1",
            [user.into()],
        ))
        .await?;
    let profile = if let Some(row) = profile_row {
        json!({
            "display_name": row.try_get::<String, _>("display_name")?,
            "locale": row.try_get::<String, _>("locale")?,
            "timezone": row.try_get::<String, _>("timezone")?,
            "notifications": row.try_get::<bool, _>("notifications")?,
            "mfa_enabled": row.try_get::<bool, _>("mfa_enabled")?,
        })
    } else {
        json!({
            "display_name": "",
            "locale": "en-US",
            "timezone": "UTC",
            "notifications": true,
            "mfa_enabled": false,
        })
    };
    let oauth_rows = transaction
        .fetch_all(Sql::bind(
            "SELECT provider, created_at FROM \"_appstruct_auth_oauth_accounts\" WHERE user_id = $1 ORDER BY created_at, provider",
            [user.into()],
        ))
        .await?;
    let oauth_accounts = oauth_rows
        .iter()
        .map(|row| {
            Ok(json!({
                "provider": row.try_get::<String, _>("provider")?,
                "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
            }))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let session_rows = transaction
        .fetch_all(Sql::bind(
            "SELECT created_at, expires_at, revoked_at FROM \"_appstruct_auth_sessions\" WHERE user_id = $1 ORDER BY created_at DESC LIMIT 500",
            [user.into()],
        ))
        .await?;
    let sessions = session_rows
        .iter()
        .map(|row| {
            Ok(json!({
                "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
                "expires_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at")?,
                "revoked_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")?,
            }))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let token_rows = transaction
        .fetch_all(Sql::bind(
            "SELECT id, name, created_at, last_used_at, expires_at, revoked_at FROM \"_appstruct_auth_api_tokens\" WHERE user_id = $1 ORDER BY created_at DESC, id DESC",
            [user.into()],
        ))
        .await?;
    let api_tokens = token_rows
        .iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<uuid::Uuid, _>("id")?,
                "name": row.try_get::<String, _>("name")?,
                "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
                "last_used_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at")?,
                "expires_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")?,
                "revoked_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")?,
            }))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let view_rows = transaction
        .fetch_all(Sql::bind(
            "SELECT id, resource, name, scope, tenant_id, state, created_at, updated_at FROM \"_appstruct_auth_saved_views\" WHERE owner_id = $1 ORDER BY created_at, id",
            [user.into()],
        ))
        .await?;
    let saved_views = view_rows
        .iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<uuid::Uuid, _>("id")?,
                "resource": row.try_get::<String, _>("resource")?,
                "name": row.try_get::<String, _>("name")?,
                "scope": row.try_get::<String, _>("scope")?,
                "tenant_id": row.try_get::<Option<uuid::Uuid>, _>("tenant_id")?,
                "state": row.try_get::<Value, _>("state")?,
                "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
                "updated_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at")?,
            }))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let notification_rows = transaction
        .fetch_all(Sql::bind(
            "SELECT id, tenant_id, title, body, target, read_at, created_at FROM \"_appstruct_product_notifications\" WHERE user_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1000",
            [user.into()],
        ))
        .await?;
    let notifications = notification_rows
        .iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<uuid::Uuid, _>("id")?,
                "tenant_id": row.try_get::<Option<uuid::Uuid>, _>("tenant_id")?,
                "title": row.try_get::<String, _>("title")?,
                "body": row.try_get::<String, _>("body")?,
                "target": row.try_get::<String, _>("target")?,
                "read_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("read_at")?,
                "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
            }))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let organizations = if config::TENANT_ENABLED {
        let org_rows = transaction
            .fetch_all(Sql::bind(
                "SELECT o.id, o.name, m.role, m.created_at FROM \"_appstruct_tenant_memberships\" m JOIN \"_appstruct_tenant_organizations\" o ON o.id = m.organization_id WHERE m.user_id = $1 ORDER BY m.created_at, o.id",
                [user.into()],
            ))
            .await?;
        org_rows
            .iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<uuid::Uuid, _>("id")?,
                    "name": row.try_get::<String, _>("name")?,
                    "role": row.try_get::<String, _>("role")?,
                    "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
                }))
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        Vec::new()
    };
    let roles: Value = account.try_get("roles")?;
    let exported = json!({
        "exported_at": chrono::Utc::now(),
        "account": {
            "id": user,
            "email": account.try_get::<String, _>("email")?,
            "roles": roles,
            "email_verified_at": account.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("email_verified_at")?,
            "has_password": account.try_get::<bool, _>("has_password")?,
            "created_at": account.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
        },
        "profile": profile,
        "oauth_accounts": oauth_accounts,
        "sessions": sessions,
        "api_tokens": api_tokens,
        "saved_views": saved_views,
        "notifications": notifications,
        "organizations": organizations,
    });
    transaction.commit().await?;
    Ok(Json(exported))
}

async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<Proof>,
) -> Result<(HeaderMap, StatusCode), ApiError> {
    let user = reauthenticate(&state, &headers, &proof).await?;
    let transaction = state.database.begin().await?;
    verify_mfa(&transaction, user, proof.code.as_deref()).await?;
    let account = transaction
        .fetch_optional(Sql::bind(
            "SELECT roles FROM \"_appstruct_auth_accounts\" WHERE user_id = $1 FOR UPDATE",
            [user.into()],
        ))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let roles: Vec<String> =
        serde_json::from_value(account.try_get("roles")?).map_err(|_| ApiError::Internal)?;
    if roles.iter().any(|role| role == "admin") {
        let counts = transaction
            .fetch_optional(Sql::bind(
                "SELECT COUNT(*) FILTER (WHERE user_id <> $1) AS others, COUNT(*) FILTER (WHERE user_id <> $1 AND roles @> '[\"admin\"]'::jsonb) AS other_admins FROM \"_appstruct_auth_accounts\"",
                [user.into()],
            ))
            .await?
            .ok_or(ApiError::Internal)?;
        let others: i64 = counts.try_get("others")?;
        let other_admins: i64 = counts.try_get("other_admins")?;
        if others > 0 && other_admins == 0 {
            return Err(ApiError::Conflict(
                "Assign another administrator before deleting the last admin account".to_owned(),
            ));
        }
    }
    if config::TENANT_ENABLED {
        prepare_tenant_account_deletion(&transaction, user).await?;
    }
    let delete_user = format!(
        "DELETE FROM {} WHERE {} = $1",
        quote_ident(config::USER_TABLE),
        quote_ident(config::USER_ID_COLUMN),
    );
    transaction
        .execute(Sql::bind(delete_user, [user.into()]))
        .await
        .map_err(map_delete_restriction)?;
    transaction.commit().await?;
    Ok((state.auth.clear_session_headers(), StatusCode::NO_CONTENT))
}

async fn prepare_tenant_account_deletion(
    transaction: &Transaction,
    user: uuid::Uuid,
) -> Result<(), ApiError> {
    let org_rows = transaction
        .fetch_all(Sql::bind(
            "SELECT o.id FROM \"_appstruct_tenant_organizations\" o WHERE o.created_by = $1 OR EXISTS (SELECT 1 FROM \"_appstruct_tenant_memberships\" m WHERE m.organization_id = o.id AND m.user_id = $1) OR EXISTS (SELECT 1 FROM \"_appstruct_tenant_invitations\" i WHERE i.organization_id = o.id AND i.invited_by = $1) ORDER BY o.id FOR NO KEY UPDATE",
            [user.into()],
        ))
        .await?;
    for row in org_rows {
        let org_id: uuid::Uuid = row.try_get("id")?;
        let stats = transaction
            .fetch_optional(Sql::bind(
                "SELECT COUNT(*) AS total, COALESCE(BOOL_OR(user_id = $2 AND role = 'owner'), false) AS is_owner FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1",
                [org_id.into(), user.into()],
            ))
            .await?
            .ok_or(ApiError::Internal)?;
        let total: i64 = stats.try_get("total")?;
        let is_owner: bool = stats.try_get("is_owner")?;
        if is_owner && total > 1 {
            return Err(ApiError::Conflict(
                "Transfer organization ownership before deleting your account".to_owned(),
            ));
        }
        if is_owner && total <= 1 {
            if config::BILLING_ENABLED {
                let active_sub = transaction
                    .fetch_optional(Sql::bind(
                        "SELECT 1 FROM \"_appstruct_billing_subscriptions\" WHERE organization_id = $1 AND status IN ('active', 'trialing') LIMIT 1",
                        [org_id.into()],
                    ))
                    .await?
                    .is_some();
                if active_sub {
                    return Err(ApiError::Conflict(
                        "Cancel active organization subscriptions before deleting your account"
                            .to_owned(),
                    ));
                }
            }
            if config::ACTIVITY_ENABLED {
                transaction
                    .execute(Sql::bind(
                        "DELETE FROM \"_appstruct_activity_entries\" WHERE tenant_id = $1",
                        [org_id.into()],
                    ))
                    .await?;
            }
            if config::REPORT_ENABLED {
                transaction
                    .execute(Sql::bind(
                        "DELETE FROM \"_appstruct_report_runs\" WHERE tenant_id = $1",
                        [org_id.into()],
                    ))
                    .await?;
            }
            transaction
                .execute(Sql::bind(
                    "DELETE FROM \"_appstruct_tenant_organizations\" WHERE id = $1",
                    [org_id.into()],
                ))
                .await
                .map_err(map_delete_restriction)?;
        } else {
            let owner_row = transaction
                .fetch_optional(Sql::bind(
                    "SELECT user_id FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id <> $2 AND role = 'owner' ORDER BY created_at, user_id LIMIT 1",
                    [org_id.into(), user.into()],
                ))
                .await?;
            if let Some(owner_row) = owner_row {
                let owner_id: uuid::Uuid = owner_row.try_get("user_id")?;
                transaction
                    .execute(Sql::bind(
                        "UPDATE \"_appstruct_tenant_organizations\" SET created_by = $2 WHERE id = $1 AND created_by = $3",
                        [org_id.into(), owner_id.into(), user.into()],
                    ))
                    .await?;
                transaction
                    .execute(Sql::bind(
                        "UPDATE \"_appstruct_tenant_invitations\" SET invited_by = $2 WHERE organization_id = $1 AND invited_by = $3",
                        [org_id.into(), owner_id.into(), user.into()],
                    ))
                    .await?;
            } else {
                transaction
                    .execute(Sql::bind(
                        "DELETE FROM \"_appstruct_tenant_invitations\" WHERE organization_id = $1 AND invited_by = $2",
                        [org_id.into(), user.into()],
                    ))
                    .await?;
            }
        }
    }
    Ok(())
}

fn map_delete_restriction(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .and_then(|db_err| db_err.code())
        .as_deref()
        == Some("23503")
    {
        ApiError::Conflict(
            "Transfer or delete records owned by this account before deleting it".to_owned(),
        )
    } else {
        ApiError::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_mfa_key, seal_with_key, totp, unseal_with_keys};
    use base64::Engine as _;

    #[test]
    fn rfc6238_sha1_vectors() {
        for (seconds, expected) in [
            (59, "287082"),
            (1111111109, "081804"),
            (1111111111, "050471"),
            (1234567890, "005924"),
            (2000000000, "279037"),
            (20000000000, "353130"),
        ] {
            assert_eq!(totp(b"12345678901234567890", seconds / 30), expected);
        }
    }

    #[test]
    fn mfa_key_rotation_opens_previous_envelopes_and_rejects_wrong_aad() {
        let old_b64 = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
        let new_b64 = base64::engine::general_purpose::STANDARD.encode([9_u8; 32]);
        let old_key = decode_mfa_key(&old_b64).unwrap();
        let new_key = decode_mfa_key(&new_b64).unwrap();
        let envelope = seal_with_key(&old_key, b"user-1", b"totp-secret").unwrap();
        assert!(unseal_with_keys(&new_key, &[], b"user-1", &envelope).is_err());
        let (opened, used_previous) =
            unseal_with_keys(&new_key, &[old_key], b"user-1", &envelope).unwrap();
        assert_eq!(opened, b"totp-secret");
        assert!(used_previous);
        assert!(unseal_with_keys(&new_key, &[], b"user-2", &envelope).is_err());
    }
}
