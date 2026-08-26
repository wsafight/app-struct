use crate::{Actor, ApiError, AppState, FieldViolation, TenantId};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
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
    Router::new().route(
        "/api/tenant/organizations",
        get(list_organizations).post(create_organization),
    )
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
