use super::render;
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use quote::quote;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.audit.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/audit.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let roles = &ir.audit.reader_roles;
    let allowed = quote! { false #(|| actor.has_role(#roles))* };
    let load = if ir.tenant.enabled {
        tenant_load()
    } else {
        global_load()
    };
    let record = record_source(
        ir.entities
            .iter()
            .any(|entity| entity.audit_enabled && entity.workflow.is_some()),
    );
    render(quote! {
        use crate::{ApiError, AppState, RequestContext};
        use appstruct_runtime::{MAX_LIST_PAGE, list_page_is_valid};
        use axum::{Json, Router, extract::{Query, State}, http::HeaderMap, routing::get};
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        use serde::{Deserialize, Serialize};

        #record

        pub fn router() -> Router<AppState> {
            Router::new().route("/api/audit/events", get(list))
        }

        #[derive(Debug, Default, Deserialize)]
        struct ListQuery {
            page: Option<u64>,
            page_size: Option<u64>,
            entity: Option<String>,
            record_id: Option<String>,
        }

        #[derive(Debug, Serialize)]
        struct AuditEvent {
            id: uuid::Uuid,
            entity: String,
            record_id: String,
            operation: String,
            actor_id: Option<uuid::Uuid>,
            tenant_id: Option<uuid::Uuid>,
            before: Option<serde_json::Value>,
            after: Option<serde_json::Value>,
            metadata: Option<serde_json::Value>,
            occurred_at: chrono::DateTime<chrono::Utc>,
        }

        #[derive(Debug, Serialize)]
        struct ListMeta { page: u64, page_size: u64, total: u64 }

        #[derive(Debug, Serialize)]
        struct ListResponse { data: Vec<AuditEvent>, meta: ListMeta }

        async fn list(
            State(state): State<AppState>,
            headers: HeaderMap,
            Query(query): Query<ListQuery>,
        ) -> Result<Json<ListResponse>, ApiError> {
            let context = state.context(&headers).await?;
            let actor = context.actor().ok_or(ApiError::Unauthorized)?;
            if !(#allowed) { return Err(ApiError::Forbidden); }
            let page = query.page.unwrap_or(1);
            let page_size = query.page_size.unwrap_or(50);
            if !list_page_is_valid(page, page_size) {
                return Err(ApiError::InvalidQuery(
                    format!("`page` must be between 1 and {MAX_LIST_PAGE} and `page_size` must be between 1 and 100")
                ));
            }
            for (name, value) in [("entity", query.entity.as_deref()), ("record_id", query.record_id.as_deref())] {
                if value.is_some_and(|value| value.is_empty() || value.len() > 255) {
                    return Err(ApiError::InvalidQuery(format!(
                        "`{name}` must contain between 1 and 255 bytes"
                    )));
                }
            }
            let offset = (page - 1).checked_mul(page_size)
                .ok_or_else(|| ApiError::InvalidQuery("pagination is too large".to_owned()))?;
            #load
            let data = rows.into_iter().map(event_from_row)
                .collect::<Result<Vec<_>, sea_orm::DbErr>>()?;
            Ok(Json(ListResponse {
                data,
                meta: ListMeta { page, page_size, total: u64::try_from(total).unwrap_or_default() },
            }))
        }

        fn event_from_row(row: sea_orm::QueryResult) -> Result<AuditEvent, sea_orm::DbErr> {
            Ok(AuditEvent {
                id: row.try_get("", "id")?, entity: row.try_get("", "entity")?,
                record_id: row.try_get("", "record_id")?, operation: row.try_get("", "operation")?,
                actor_id: row.try_get("", "actor_id")?, tenant_id: row.try_get("", "tenant_id")?,
                before: row.try_get("", "before")?, after: row.try_get("", "after")?,
                metadata: row.try_get("", "metadata")?,
                occurred_at: row.try_get("", "occurred_at")?,
            })
        }
    })
}

fn record_source(workflow: bool) -> proc_macro2::TokenStream {
    let workflow_record = workflow.then(|| quote! {
        pub async fn record_with_metadata<C, T>(
            database: &C,
            context: &RequestContext<'_>,
            entity: &str,
            record_id: String,
            operation: &str,
            before: Option<&T>,
            after: Option<&T>,
            metadata: &serde_json::Value,
        ) -> Result<(), ApiError>
        where
            C: ConnectionTrait,
            T: Serialize,
        {
            let before = before.map(serde_json::to_value).transpose().map_err(|_| ApiError::Internal)?;
            let after = after.map(serde_json::to_value).transpose().map_err(|_| ApiError::Internal)?;
            insert(
                database, context, entity, record_id, operation, before, after,
                Some(metadata.clone()),
            ).await
        }
    });
    quote! {
        pub async fn record<C, T>(
            database: &C,
            context: &RequestContext<'_>,
            entity: &str,
            record_id: String,
            operation: &str,
            before: Option<&T>,
            after: Option<&T>,
        ) -> Result<(), ApiError>
        where
            C: ConnectionTrait,
            T: Serialize,
        {
            let before = before.map(serde_json::to_value).transpose().map_err(|_| ApiError::Internal)?;
            let after = after.map(serde_json::to_value).transpose().map_err(|_| ApiError::Internal)?;
            insert(database, context, entity, record_id, operation, before, after, None).await
        }

        #workflow_record

        async fn insert<C: ConnectionTrait>(
            database: &C,
            context: &RequestContext<'_>,
            entity: &str,
            record_id: String,
            operation: &str,
            before: Option<serde_json::Value>,
            after: Option<serde_json::Value>,
            metadata: Option<serde_json::Value>,
        ) -> Result<(), ApiError> {
            let actor_id = context.actor().map(|actor| actor.id);
            database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_audit_events\" (id, entity, record_id, operation, actor_id, tenant_id, before, after, metadata, occurred_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, CURRENT_TIMESTAMP)",
                [
                    uuid::Uuid::now_v7().into(), entity.to_owned().into(), record_id.into(),
                    operation.to_owned().into(), actor_id.into(), context.tenant().into(),
                    before.into(), after.into(), metadata.into(),
                ],
            )).await?;
            Ok(())
        }
    }
}

fn tenant_load() -> proc_macro2::TokenStream {
    quote! {
        let tenant = context.require_tenant()?;
        let count = state.database.query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT COUNT(*) AS total FROM \"_appstruct_audit_events\" WHERE tenant_id = $1 AND ($2::text IS NULL OR entity = $2) AND ($3::text IS NULL OR record_id = $3)",
            [tenant.into(), query.entity.clone().into(), query.record_id.clone().into()],
        )).await?.ok_or(ApiError::Internal)?;
        let total: i64 = count.try_get("", "total")?;
        let rows = state.database.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id, entity, record_id, operation, actor_id, tenant_id, before, after, metadata, occurred_at FROM \"_appstruct_audit_events\" WHERE tenant_id = $1 AND ($2::text IS NULL OR entity = $2) AND ($3::text IS NULL OR record_id = $3) ORDER BY occurred_at DESC, id DESC LIMIT $4 OFFSET $5",
            [tenant.into(), query.entity.clone().into(), query.record_id.clone().into(), i64::try_from(page_size).unwrap_or(100).into(), i64::try_from(offset).unwrap_or(i64::MAX).into()],
        )).await?;
    }
}

fn global_load() -> proc_macro2::TokenStream {
    quote! {
        let count = state.database.query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT COUNT(*) AS total FROM \"_appstruct_audit_events\" WHERE ($1::text IS NULL OR entity = $1) AND ($2::text IS NULL OR record_id = $2)",
            [query.entity.clone().into(), query.record_id.clone().into()],
        )).await?.ok_or(ApiError::Internal)?;
        let total: i64 = count.try_get("", "total")?;
        let rows = state.database.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id, entity, record_id, operation, actor_id, tenant_id, before, after, metadata, occurred_at FROM \"_appstruct_audit_events\" WHERE ($1::text IS NULL OR entity = $1) AND ($2::text IS NULL OR record_id = $2) ORDER BY occurred_at DESC, id DESC LIMIT $3 OFFSET $4",
            [query.entity.clone().into(), query.record_id.clone().into(), i64::try_from(page_size).unwrap_or(100).into(), i64::try_from(offset).unwrap_or(i64::MAX).into()],
        )).await?;
    }
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use crate::AppState;
        use axum::Router;
        pub fn router() -> Router<AppState> { Router::new() }
    })
}
