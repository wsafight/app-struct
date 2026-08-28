use crate::{Actor, ApiError, AppState, FieldViolation, TenantId};
use crate::auth::{random_token, token_hash};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};

const TENANT_HEADER: &str = "x-appstruct-tenant";

pub async fn resolve(
    database: &DatabaseConnection,
    headers: &HeaderMap,
    actor: Option<&Actor>,
) -> Result<Option<TenantId>, ApiError> {
    let Some(raw) = headers.get(TENANT_HEADER) else {
        return Ok(None);
    };
    let tenant = raw
        .to_str()
        .ok()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(ApiError::InvalidTenant)?;
    let actor = actor.ok_or(ApiError::Unauthorized)?;
    let member = database
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2",
            [tenant.into(), actor.id.into()],
        ))
        .await?
        .is_some();
    if member {
        Ok(Some(tenant))
    } else {
        Err(ApiError::Forbidden)
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tenant/organizations", get(list_organizations).post(create_organization))
        .route("/api/tenant/invitations", get(list_invitations).post(create_invitation))
        .route("/api/tenant/invitations/{id}", delete(revoke_invitation))
        .route("/api/tenant/invitations/{token}/accept", post(accept_invitation))
}

#[derive(Debug, Serialize)]
struct Organization {
    id: TenantId,
    name: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct OrganizationList {
    data: Vec<Organization>,
}

#[derive(Debug, Deserialize)]
struct CreateOrganization {
    name: String,
}

#[derive(Debug, Serialize)]
struct Invitation {
    id: uuid::Uuid,
    email: String,
    role: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct InvitationList {
    data: Vec<Invitation>,
}

#[derive(Debug, Deserialize)]
struct CreateInvitation {
    email: String,
    #[serde(default = "default_member_role")]
    role: String,
}

fn default_member_role() -> String { "member".to_owned() }

async fn list_organizations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrganizationList>, ApiError> {
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let rows = state
        .database
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT o.id, o.name, m.role, o.created_at FROM \"_appstruct_tenant_organizations\" o JOIN \"_appstruct_tenant_memberships\" m ON m.organization_id = o.id WHERE m.user_id = $1 ORDER BY o.name, o.id",
            [actor.id.into()],
        ))
        .await?;
    let data = rows
        .into_iter()
        .map(|row| {
            Ok(Organization {
                id: row.try_get("", "id")?,
                name: row.try_get("", "name")?,
                role: row.try_get("", "role")?,
                created_at: row.try_get("", "created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sea_orm::DbErr>>()?;
    Ok(Json(OrganizationList { data }))
}

async fn create_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateOrganization>,
) -> Result<(StatusCode, Json<Organization>), ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let actor = state
        .auth
        .actor(&state.database, &headers)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::Validation(vec![FieldViolation {
            field: "name".to_owned(),
            message: "Name must contain between 1 and 120 bytes".to_owned(),
        }]));
    }
    let id = uuid::Uuid::now_v7();
    let transaction = state.database.begin().await?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_tenant_organizations\" (id, name, created_by, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
            [id.into(), name.to_owned().into(), actor.id.into()],
        ))
        .await?;
    transaction
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO \"_appstruct_tenant_memberships\" (organization_id, user_id, role, created_at) VALUES ($1, $2, 'owner', CURRENT_TIMESTAMP)",
            [id.into(), actor.id.into()],
        ))
        .await?;
    transaction.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Organization {
            id,
            name: name.to_owned(),
            role: "owner".to_owned(),
            created_at: chrono::Utc::now(),
        }),
    ))
}

async fn list_invitations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<InvitationList>, ApiError> {
    let (actor, tenant) = current_owner(&state, &headers).await?;
    let _ = actor;
    let rows = state.database.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, email, role, expires_at, accepted_at, created_at FROM \"_appstruct_tenant_invitations\" WHERE organization_id = $1 ORDER BY created_at DESC, id DESC",
        [tenant.into()],
    )).await?;
    let data = rows.into_iter().map(invitation_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(InvitationList { data }))
}

async fn create_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateInvitation>,
) -> Result<(StatusCode, Json<Invitation>), ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let (actor, tenant) = current_owner(&state, &headers).await?;
    let email = normalize_email(&input.email)?;
    if input.role != "member" {
        return Err(ApiError::Validation(vec![FieldViolation {
            field: "role".to_owned(),
            message: "Only the member role can be invited".to_owned(),
        }]));
    }
    let token = random_token();
    let invitation_id = uuid::Uuid::now_v7();
    let transaction = state.database.begin().await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "DELETE FROM \"_appstruct_tenant_invitations\" WHERE organization_id = $1 AND LOWER(email) = $2 AND accepted_at IS NULL",
        [tenant.into(), email.clone().into()],
    )).await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_tenant_invitations\" (id, organization_id, email, role, token_hash, expires_at, invited_by, created_at) VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP + INTERVAL '7 days', $6, CURRENT_TIMESTAMP)",
        [
            invitation_id.into(), tenant.into(), email.clone().into(), input.role.clone().into(),
            token_hash(&token).into(), actor.id.into(),
        ],
    )).await?;
    transaction.commit().await?;
    let invitation_url = format!("{}/accept-invitation?token={token}", state.auth.config.frontend_url);
    if let Err(error) = state.auth.mail.send_invitation(&state.database, &email, &invitation_url).await {
        tracing::warn!(?error, "organization invitation email was not delivered");
    }
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    Ok((StatusCode::CREATED, Json(Invitation {
        id: invitation_id, email, role: input.role, expires_at, accepted_at: None,
        created_at: chrono::Utc::now(),
    })))
}

async fn revoke_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let (_actor, tenant) = current_owner(&state, &headers).await?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
    state.database.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "DELETE FROM \"_appstruct_tenant_invitations\" WHERE id = $1 AND organization_id = $2 AND accepted_at IS NULL",
        [id.into(), tenant.into()],
    )).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn accept_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Result<Json<Organization>, ApiError> {
    state.auth.verify_csrf(&state.database, &headers).await?;
    let actor = state.auth.actor(&state.database, &headers).await?.ok_or(ApiError::Unauthorized)?;
    let transaction = state.database.begin().await?;
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, organization_id, email, role FROM \"_appstruct_tenant_invitations\" WHERE token_hash = $1 AND accepted_at IS NULL AND expires_at > CURRENT_TIMESTAMP FOR UPDATE",
        [token_hash(&token).into()],
    )).await?.ok_or(ApiError::InvalidInvitationToken)?;
    let invitation_id: uuid::Uuid = row.try_get("", "id")?;
    let organization_id: uuid::Uuid = row.try_get("", "organization_id")?;
    let email: String = row.try_get("", "email")?;
    let role: String = row.try_get("", "role")?;
    if actor.email.to_ascii_lowercase() != email.to_ascii_lowercase() {
        return Err(ApiError::Forbidden);
    }
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO \"_appstruct_tenant_memberships\" (organization_id, user_id, role, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP) ON CONFLICT (organization_id, user_id) DO NOTHING",
            [organization_id.into(), actor.id.into(), role.clone().into()],
    )).await?;
    transaction.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE \"_appstruct_tenant_invitations\" SET accepted_at = CURRENT_TIMESTAMP WHERE id = $1",
        [invitation_id.into()],
    )).await?;
    let row = transaction.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, name, created_at FROM \"_appstruct_tenant_organizations\" WHERE id = $1",
        [organization_id.into()],
    )).await?.ok_or(ApiError::NotFound)?;
    let organization = Organization {
        id: row.try_get("", "id")?, name: row.try_get("", "name")?, role,
        created_at: row.try_get("", "created_at")?,
    };
    transaction.commit().await?;
    Ok(Json(organization))
}

async fn current_owner(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(Actor, TenantId), ApiError> {
    let context = state.context(headers).await?;
    let actor = context.actor().cloned().ok_or(ApiError::Unauthorized)?;
    let tenant = context.require_tenant()?;
    let member = state.database.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT 1 FROM \"_appstruct_tenant_memberships\" WHERE organization_id = $1 AND user_id = $2 AND role = 'owner'",
        [tenant.into(), actor.id.into()],
    )).await?.is_some();
    if member { Ok((actor, tenant)) } else { Err(ApiError::Forbidden) }
}

fn invitation_from_row(row: sea_orm::QueryResult) -> Result<Invitation, sea_orm::DbErr> {
    Ok(Invitation {
        id: row.try_get("", "id")?, email: row.try_get("", "email")?, role: row.try_get("", "role")?,
        expires_at: row.try_get("", "expires_at")?, accepted_at: row.try_get("", "accepted_at")?,
        created_at: row.try_get("", "created_at")?,
    })
}

fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_ascii_lowercase();
    if email.len() > 320 || !email.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.')) {
        return Err(ApiError::InvalidCredentialsInput);
    }
    Ok(email)
}
