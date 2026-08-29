use super::config;
use super::session::{cookie_value, random_token, token_hash};
use crate::{Actor, ApiError, AppState};
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
#[allow(unused_imports)]
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
#[allow(unused_imports)]
use axum::response::Redirect;
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
        .route("/api/auth/email/request", post(request_email_verification))
        .route("/api/auth/email/verify", post(verify_email))
        // appstruct:oauth:start
        .route("/api/auth/oauth/oidc/start", get(start_oidc))
        .route("/api/auth/oauth/oidc/callback", get(oidc_callback))
        // appstruct:oauth:end
        .route("/api/auth/tokens", get(list_api_tokens).post(create_api_token))
        .route("/api/auth/tokens/{id}", axum::routing::delete(revoke_api_token))
        .route("/api/admin/overview", get(admin_overview))
        .route("/api/admin/users", get(list_admin_users))
        .route("/api/admin/users/{id}/revoke-sessions", post(revoke_admin_user_sessions))
        .route("/api/admin/jobs", get(list_admin_jobs))
        .route("/api/admin/jobs/{id}/retry", post(retry_admin_job))
        .route("/api/admin/jobs/{id}/replay", post(replay_admin_job))
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

#[derive(Deserialize)]
struct VerifyEmailInput {
    token: String,
}

// appstruct:oauth:start
#[derive(Deserialize)]
struct OidcCallback {
    code: String,
    state: String,
}
// appstruct:oauth:end

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
struct AdminOverview {
    users: i64,
    organizations: i64,
    invitations: i64,
    sessions: i64,
    jobs_queued: i64,
    jobs_dead: i64,
    mail_deliveries: i64,
    files: i64,
    audit_events: i64,
}

#[derive(Serialize)]
struct AdminUser {
    id: uuid::Uuid,
    email: String,
    roles: Vec<String>,
    email_verified: bool,
    active_sessions: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct AdminUserList {
    data: Vec<AdminUser>,
}

#[derive(Serialize)]
struct AdminSessionRevocation {
    revoked: u64,
}

#[derive(Deserialize)]
struct AdminUserQuery {
    limit: Option<u64>,
}

#[derive(Deserialize)]
struct AdminJobQuery {
    status: Option<String>,
    limit: Option<u64>,
}

#[derive(Serialize)]
struct AdminJob {
    id: uuid::Uuid,
    queue: String,
    kind: String,
    status: String,
    tenant_id: Option<uuid::Uuid>,
    attempts: i32,
    max_attempts: i32,
    run_at: chrono::DateTime<chrono::Utc>,
    last_error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct AdminJobList {
    data: Vec<AdminJob>,
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

async fn request_email_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if account_email_verified(&state, actor.id).await? {
        return Ok(StatusCode::NO_CONTENT);
    }
    issue_email_verification(&state, actor.id, &actor.email).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn verify_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<VerifyEmailInput>,
) -> Result<StatusCode, ApiError> {
    state.auth.validate_origin(&headers)?;
    let transaction = state.database.begin().await?;
    let row = transaction
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT user_id FROM \"_appstruct_auth_email_verifications\" WHERE token_hash = $1 AND used_at IS NULL AND expires_at > CURRENT_TIMESTAMP FOR UPDATE",
            [token_hash(&input.token).into()],
        ))
        .await?
        .ok_or(ApiError::InvalidEmailVerificationToken)?;
    let user_id: uuid::Uuid = row.try_get("", "user_id")?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_accounts\" SET email_verified_at = CURRENT_TIMESTAMP WHERE user_id = $1",
            [user_id.into()],
        ))
        .await?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_email_verifications\" SET used_at = CURRENT_TIMESTAMP WHERE token_hash = $1",
            [token_hash(&input.token).into()],
        ))
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// appstruct:oauth:start
async fn start_oidc() -> Result<(HeaderMap, Redirect), ApiError> {
    if !config::OAUTH_ENABLED {
        return Err(ApiError::NotFound);
    }
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
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<OidcCallback>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    if !config::OAUTH_ENABLED {
        return Err(ApiError::NotFound);
    }
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
        .send()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    if !token_response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let token_body: serde_json::Value = token_response.json().await.map_err(|_| ApiError::OAuthProvider)?;
    let access_token = token_body
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::OAuthProvider)?;
    let userinfo_url = required_env("APPSTRUCT_OIDC_USERINFO_URL")?;
    let userinfo_response = reqwest::Client::new()
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| ApiError::OAuthProvider)?;
    if !userinfo_response.status().is_success() {
        return Err(ApiError::OAuthProvider);
    }
    let claims: serde_json::Value = userinfo_response.json().await.map_err(|_| ApiError::OAuthProvider)?;
    let subject = claims
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::OAuthProvider)?;
    let email = normalize_email(
        claims
            .get("email")
            .and_then(serde_json::Value::as_str)
            .ok_or(ApiError::OAuthProvider)?,
    )?;
    let user_id = find_or_create_oauth_user(&state, subject, &email).await?;
    let (session, csrf) = state.auth.create_session(&state.database, user_id).await?;
    let mut response_headers = state.auth.session_headers(&session, &csrf);
    response_headers.append(
        axum::http::header::SET_COOKIE,
        "appstruct_oidc_state=; Path=/api/auth/oauth/oidc; Max-Age=0; HttpOnly; SameSite=Lax"
            .parse()
            .map_err(|_| ApiError::Internal)?,
    );
    Ok((response_headers, Redirect::temporary("/")))
}
// appstruct:oauth:end

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

async fn admin_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminOverview>, ApiError> {
    require_admin(&state, &headers).await?;
    Ok(Json(AdminOverview {
        users: count_table(&state, config::USER_TABLE).await?,
        organizations: optional_count(&state, config::TENANT_ENABLED, "_appstruct_tenant_organizations").await?,
        invitations: optional_count(&state, config::TENANT_ENABLED, "_appstruct_tenant_invitations").await?,
        sessions: count_table(&state, "_appstruct_auth_sessions").await?,
        jobs_queued: optional_count_where(&state, config::JOBS_ENABLED, "_appstruct_jobs", "status = 'queued'").await?,
        jobs_dead: optional_count_where(&state, config::JOBS_ENABLED, "_appstruct_jobs", "status = 'dead'").await?,
        mail_deliveries: optional_count(&state, config::MAIL_ENABLED, "_appstruct_mail_deliveries").await?,
        files: optional_count(&state, config::FILE_ENABLED, "_appstruct_files").await?,
        audit_events: optional_count(&state, config::AUDIT_ENABLED, "_appstruct_audit_events").await?,
    }))
}

async fn list_admin_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<AdminUserQuery>,
) -> Result<Json<AdminUserList>, ApiError> {
    require_admin(&state, &headers).await?;
    let limit = input.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::InvalidQuery(
            "limit must be between 1 and 100".to_owned(),
        ));
    }
    let sql = format!(
        "SELECT u.{id} AS id, u.{email} AS email, a.roles, a.email_verified_at, a.created_at, (SELECT COUNT(*) FROM \"_appstruct_auth_sessions\" s WHERE s.user_id = a.user_id AND s.revoked_at IS NULL AND s.expires_at > CURRENT_TIMESTAMP) AS active_sessions FROM {users} u JOIN \"_appstruct_auth_accounts\" a ON a.user_id = u.{id} ORDER BY a.created_at DESC, u.{id} DESC LIMIT $1",
        id = quote_ident(config::USER_ID_COLUMN),
        email = quote_ident(config::USER_EMAIL_COLUMN),
        users = quote_ident(config::USER_TABLE),
    );
    let rows = state
        .database
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [i64::try_from(limit).unwrap_or(100).into()],
        ))
        .await?;
    let data = rows
        .into_iter()
        .map(admin_user_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminUserList { data }))
}

async fn revoke_admin_user_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AdminSessionRevocation>, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    require_admin(&state, &headers).await?;
    let user_id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let user_exists = format!(
        "SELECT 1 FROM \"_appstruct_auth_accounts\" WHERE user_id = $1 AND EXISTS (SELECT 1 FROM {users} WHERE {users}.{id} = $1)",
        users = quote_ident(config::USER_TABLE),
        id = quote_ident(config::USER_ID_COLUMN),
    );
    if state
        .database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            user_exists,
            [user_id.into()],
        ))
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound);
    }
    let result = state
        .database
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE \"_appstruct_auth_sessions\" SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND revoked_at IS NULL",
            [user_id.into()],
        ))
        .await?;
    Ok(Json(AdminSessionRevocation {
        revoked: result.rows_affected(),
    }))
}

async fn list_admin_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<AdminJobQuery>,
) -> Result<Json<AdminJobList>, ApiError> {
    require_admin(&state, &headers).await?;
    if !config::JOBS_ENABLED {
        return Err(ApiError::NotFound);
    }
    if input.status.as_deref().is_some_and(|status| {
        !matches!(status, "queued" | "running" | "succeeded" | "dead")
    }) {
        return Err(ApiError::InvalidQuery(
            "status must be queued, running, succeeded, or dead".to_owned(),
        ));
    }
    let limit = input.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::InvalidQuery(
            "limit must be between 1 and 100".to_owned(),
        ));
    }
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, queue, kind, status::text AS status, tenant_id, attempts, max_attempts, run_at, last_error, created_at, completed_at FROM \"_appstruct_jobs\" WHERE ($1::text IS NULL OR status::text = $1) ORDER BY created_at DESC, id DESC LIMIT $2",
        [input.status.into(), i64::try_from(limit).unwrap_or(100).into()],
    )).await?;
    let data = rows.into_iter().map(admin_job_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminJobList { data }))
}

async fn retry_admin_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AdminJob>, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    require_admin(&state, &headers).await?;
    ensure_jobs_enabled()?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    let status = lock_admin_job(&transaction, id).await?;
    if status != "dead" {
        return Err(ApiError::Conflict(
            "Only dead jobs can be retried".to_owned(),
        ));
    }
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_jobs\" SET status = 'queued', attempts = 0, run_at = CURRENT_TIMESTAMP, locked_by = NULL, locked_until = NULL, last_error = NULL, completed_at = NULL WHERE id = $1 RETURNING id, queue, kind, status::text AS status, tenant_id, attempts, max_attempts, run_at, last_error, created_at, completed_at",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let job = admin_job_from_row(row)?;
    transaction.commit().await?;
    Ok(Json(job))
}

async fn replay_admin_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<AdminJob>), ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    require_admin(&state, &headers).await?;
    ensure_jobs_enabled()?;
    let source_id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    let transaction = state.database.begin().await?;
    let status = lock_admin_job(&transaction, source_id).await?;
    if !matches!(status.as_str(), "succeeded" | "dead") {
        return Err(ApiError::Conflict(
            "Only succeeded or dead jobs can be replayed".to_owned(),
        ));
    }
    let id = uuid::Uuid::now_v7();
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_jobs\" (id, queue, kind, payload, idempotency_key, tenant_id, status, attempts, max_attempts, backoff_seconds, run_at, created_at) SELECT $1, queue, kind, payload, NULL, tenant_id, 'queued', 0, max_attempts, backoff_seconds, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP FROM \"_appstruct_jobs\" WHERE id = $2 RETURNING id, queue, kind, status::text AS status, tenant_id, attempts, max_attempts, run_at, last_error, created_at, completed_at",
        [id.into(), source_id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let job = admin_job_from_row(row)?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(job)))
}

async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let actor = state.auth.actor(&state.database, headers).await?
        .ok_or(ApiError::Unauthorized)?;
    if actor.has_role("admin") { Ok(()) } else { Err(ApiError::Forbidden) }
}

fn ensure_jobs_enabled() -> Result<(), ApiError> {
    if config::JOBS_ENABLED { Ok(()) } else { Err(ApiError::NotFound) }
}

async fn lock_admin_job<C: ConnectionTrait>(
    database: &C,
    id: uuid::Uuid,
) -> Result<String, ApiError> {
    let row = database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT status::text AS status FROM \"_appstruct_jobs\" WHERE id = $1 FOR UPDATE",
        [id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    Ok(row.try_get("", "status")?)
}

fn admin_job_from_row(row: sea_orm::QueryResult) -> Result<AdminJob, sea_orm::DbErr> {
    Ok(AdminJob {
        id: row.try_get("", "id")?, queue: row.try_get("", "queue")?,
        kind: row.try_get("", "kind")?, status: row.try_get("", "status")?,
        tenant_id: row.try_get("", "tenant_id")?, attempts: row.try_get("", "attempts")?,
        max_attempts: row.try_get("", "max_attempts")?, run_at: row.try_get("", "run_at")?,
        last_error: row.try_get("", "last_error")?, created_at: row.try_get("", "created_at")?,
        completed_at: row.try_get("", "completed_at")?,
    })
}

fn admin_user_from_row(row: sea_orm::QueryResult) -> Result<AdminUser, sea_orm::DbErr> {
    let roles: serde_json::Value = row.try_get("", "roles")?;
    Ok(AdminUser {
        id: row.try_get("", "id")?,
        email: row.try_get("", "email")?,
        roles: serde_json::from_value(roles)
            .map_err(|error| sea_orm::DbErr::Type(error.to_string()))?,
        email_verified: row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "email_verified_at")?
            .is_some(),
        active_sessions: row.try_get("", "active_sessions")?,
        created_at: row.try_get("", "created_at")?,
    })
}

async fn count_table(state: &AppState, table: &str) -> Result<i64, ApiError> {
    let sql = format!("SELECT COUNT(*) AS count FROM {}", quote_ident(table));
    let row = state.database.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, [])).await?.ok_or(ApiError::Internal)?;
    Ok(row.try_get("", "count")?)
}

async fn optional_count(state: &AppState, enabled: bool, table: &str) -> Result<i64, ApiError> {
    if enabled { count_table(state, table).await } else { Ok(0) }
}

async fn optional_count_where(state: &AppState, enabled: bool, table: &str, predicate: &str) -> Result<i64, ApiError> {
    if !enabled { return Ok(0); }
    let sql = format!("SELECT COUNT(*) AS count FROM {} WHERE {}", quote_ident(table), predicate);
    let row = state.database.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, [])).await?.ok_or(ApiError::Internal)?;
    Ok(row.try_get("", "count")?)
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
    state.auth.check_login_rate(&state.database, &format!("reset:{email}")).await?;
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

async fn account_email_verified(state: &AppState, user_id: uuid::Uuid) -> Result<bool, ApiError> {
    Ok(state
        .database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT email_verified_at FROM \"_appstruct_auth_accounts\" WHERE user_id = $1",
            [user_id.into()],
        ))
        .await?
        .and_then(|row| {
            row.try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "email_verified_at")
                .ok()
                .flatten()
        })
        .is_some())
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

async fn issue_email_verification(
    state: &AppState,
    user_id: uuid::Uuid,
    email: &str,
) -> Result<(), ApiError> {
    let token = random_token();
    state
        .database
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM \"_appstruct_auth_email_verifications\" WHERE user_id = $1 AND used_at IS NULL",
            [user_id.into()],
        ))
        .await?;
    state
        .database
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_email_verifications\" (token_hash, user_id, expires_at, created_at) VALUES ($1, $2, CURRENT_TIMESTAMP + INTERVAL '24 hours', CURRENT_TIMESTAMP)",
            [token_hash(&token).into(), user_id.into()],
        ))
        .await?;
    let url = format!("{}/verify-email?token={token}", state.auth.config.frontend_url);
    if let Err(error) = state
        .auth
        .mail
        .send_email_verification(&state.database, email, &url)
        .await
    {
        tracing::warn!(?error, "email verification delivery failed");
    }
    Ok(())
}

// appstruct:oauth:start
async fn find_or_create_oauth_user(
    state: &AppState,
    subject: &str,
    email: &str,
) -> Result<uuid::Uuid, ApiError> {
    let provider_subject = format!("oidc:{subject}");
    if let Some(row) = state
        .database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT user_id FROM \"_appstruct_auth_oauth_accounts\" WHERE provider = $1 AND subject = $2",
            ["oidc".to_owned().into(), provider_subject.clone().into()],
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
                [
                    id.into(),
                    hash_password(&random_token())?.into(),
                    serde_json::json!([config::DEFAULT_ROLE]).into(),
                ],
            ))
            .await?;
        id
    };
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_auth_oauth_accounts\" (provider, subject, user_id, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
            ["oidc".to_owned().into(), provider_subject.into(), user_id.into()],
        ))
        .await?;
    transaction.commit().await?;
    Ok(user_id)
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
// appstruct:oauth:end

pub(super) fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
