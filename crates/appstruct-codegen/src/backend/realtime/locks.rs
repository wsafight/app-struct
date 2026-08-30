use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn support() -> TokenStream {
    let declarations = declarations();
    let status = status();
    let acquire = acquire();
    let renewal = renewal();
    let helpers = helpers();
    quote! {
        #declarations
        #status
        #acquire
        #renewal
        #helpers
    }
}

fn declarations() -> TokenStream {
    quote! {
        #[derive(Debug, Deserialize)]
        struct LockRequest { ttl_seconds: Option<u64> }

        #[derive(Clone, Debug, Serialize)]
        pub struct RealtimeLockLease {
            pub lease_token: uuid::Uuid,
            pub actor_id: uuid::Uuid,
            pub tenant_id: Option<uuid::Uuid>,
            pub resource: String,
            pub record_id: String,
            pub acquired_at: chrono::DateTime<chrono::Utc>,
            pub expires_at: chrono::DateTime<chrono::Utc>,
        }
        #[derive(Serialize)]
        struct LockStatus { data: Option<RealtimeLockLease> }
    }
}

fn status() -> TokenStream {
    quote! {
        async fn lock_status(
            State(state): State<AppState>, headers: HeaderMap, Query(query): Query<RealtimeQuery>,
        ) -> Result<axum::Json<LockStatus>, ApiError> {
            let record_id = require_lock_scope(&query)?.to_owned();
            let context = scoped_context(&state, headers, query.tenant_id).await?;
            if context.actor().is_none() { return Err(ApiError::Unauthorized); }
            let resource = query.resource.as_deref().ok_or_else(|| {
                ApiError::InvalidQuery("realtime resource is required".to_owned())
            })?;
            authorize_resource_scope(&state, &context, resource, Some(&record_id)).await?;
            let key = lock_key(context.tenant(), resource, &record_id);
            let row = state.database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT lease_token, actor_id, tenant_id, resource, record_id, acquired_at, expires_at FROM \"_appstruct_realtime_locks\" WHERE lock_key = $1 AND expires_at > CURRENT_TIMESTAMP",
                [key.into()],
            )).await?;
            let data = row.map(lock_from_row).transpose()?;
            Ok(axum::Json(LockStatus { data }))
        }
    }
}

fn acquire() -> TokenStream {
    quote! {
        async fn acquire_lock(
            State(state): State<AppState>, headers: HeaderMap, Query(query): Query<RealtimeQuery>,
            axum::Json(input): axum::Json<LockRequest>,
        ) -> Result<(axum::http::StatusCode, axum::Json<RealtimeLockLease>), ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let record_id = require_lock_scope(&query)?.to_owned();
            let ttl = lock_ttl(input.ttl_seconds)?;
            let context = scoped_context(&state, headers, query.tenant_id).await?;
            let actor = context.actor().ok_or(ApiError::Unauthorized)?;
            let resource = query.resource.as_deref().ok_or_else(|| {
                ApiError::InvalidQuery("realtime resource is required".to_owned())
            })?;
            authorize_resource_scope(&state, &context, resource, Some(&record_id)).await?;
            let key = lock_key(context.tenant(), resource, &record_id);
            let token = uuid::Uuid::now_v7();
            let row = state.database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_realtime_locks\" AS lease (lock_key, lease_token, actor_id, tenant_id, resource, record_id, acquired_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + ($7 * INTERVAL '1 second')) ON CONFLICT (lock_key) DO UPDATE SET lease_token = EXCLUDED.lease_token, actor_id = EXCLUDED.actor_id, tenant_id = EXCLUDED.tenant_id, resource = EXCLUDED.resource, record_id = EXCLUDED.record_id, acquired_at = CURRENT_TIMESTAMP, expires_at = EXCLUDED.expires_at WHERE lease.expires_at <= CURRENT_TIMESTAMP RETURNING lease_token, actor_id, tenant_id, resource, record_id, acquired_at, expires_at",
                [key.into(), token.into(), actor.id.into(), context.tenant().into(),
                 resource.to_owned().into(), record_id.into(), ttl.into()],
            )).await?;
            let row = row.ok_or_else(|| ApiError::Conflict(
                "The record already has an active edit lease".to_owned(),
            ))?;
            Ok((axum::http::StatusCode::CREATED, axum::Json(lock_from_row(row)?)))
        }
    }
}

fn renewal() -> TokenStream {
    quote! {
        async fn renew_lock(
            State(state): State<AppState>, axum::extract::Path(token): axum::extract::Path<String>,
            headers: HeaderMap, Query(query): Query<RealtimeQuery>,
            axum::Json(input): axum::Json<LockRequest>,
        ) -> Result<axum::Json<RealtimeLockLease>, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let token = token.parse::<uuid::Uuid>().map_err(|_| ApiError::InvalidId)?;
            let record_id = require_lock_scope(&query)?.to_owned();
            let ttl = lock_ttl(input.ttl_seconds)?;
            let context = scoped_context(&state, headers, query.tenant_id).await?;
            let actor = context.actor().ok_or(ApiError::Unauthorized)?;
            let resource = query.resource.as_deref().ok_or_else(|| {
                ApiError::InvalidQuery("realtime resource is required".to_owned())
            })?;
            authorize_resource_scope(&state, &context, resource, Some(&record_id)).await?;
            let key = lock_key(context.tenant(), resource, &record_id);
            let row = state.database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_realtime_locks\" SET expires_at = CURRENT_TIMESTAMP + ($4 * INTERVAL '1 second') WHERE lock_key = $1 AND lease_token = $2 AND actor_id = $3 AND expires_at > CURRENT_TIMESTAMP RETURNING lease_token, actor_id, tenant_id, resource, record_id, acquired_at, expires_at",
                [key.into(), token.into(), actor.id.into(), ttl.into()],
            )).await?;
            let row = row.ok_or_else(|| ApiError::Conflict(
                "The edit lease expired or is owned by another actor".to_owned(),
            ))?;
            Ok(axum::Json(lock_from_row(row)?))
        }

        async fn release_lock(
            State(state): State<AppState>, axum::extract::Path(token): axum::extract::Path<String>,
            headers: HeaderMap, Query(query): Query<RealtimeQuery>,
        ) -> Result<axum::http::StatusCode, ApiError> {
            state.auth.verify_csrf(&state.database, &headers).await?;
            let token = token.parse::<uuid::Uuid>().map_err(|_| ApiError::InvalidId)?;
            let record_id = require_lock_scope(&query)?.to_owned();
            let context = scoped_context(&state, headers, query.tenant_id).await?;
            let actor = context.actor().ok_or(ApiError::Unauthorized)?;
            let resource = query.resource.as_deref().ok_or_else(|| {
                ApiError::InvalidQuery("realtime resource is required".to_owned())
            })?;
            authorize_resource_scope(&state, &context, resource, Some(&record_id)).await?;
            let key = lock_key(context.tenant(), resource, &record_id);
            let result = state.database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "DELETE FROM \"_appstruct_realtime_locks\" WHERE lock_key = $1 AND lease_token = $2 AND actor_id = $3",
                [key.into(), token.into(), actor.id.into()],
            )).await?;
            if result.rows_affected() == 1 { Ok(axum::http::StatusCode::NO_CONTENT) }
            else { Err(ApiError::NotFound) }
        }
    }
}

fn helpers() -> TokenStream {
    quote! {
        fn require_lock_scope(query: &RealtimeQuery) -> Result<&str, ApiError> {
            validate_scope(query)?;
            query.record_id.as_deref().ok_or_else(|| {
                ApiError::InvalidQuery("record_id is required for an edit lease".to_owned())
            })
        }
        fn lock_ttl(ttl: Option<u64>) -> Result<i64, ApiError> {
            let ttl = ttl.unwrap_or(30);
            if !(5..=300).contains(&ttl) {
                return Err(ApiError::InvalidQuery(
                    "lock ttl_seconds must be between 5 and 300".to_owned(),
                ));
            }
            i64::try_from(ttl).map_err(|_| ApiError::InvalidQuery("invalid lock TTL".to_owned()))
        }
        fn lock_key(tenant_id: Option<uuid::Uuid>, resource: &str, record_id: &str) -> String {
            let tenant = tenant_id.map_or_else(|| "global".to_owned(), |id| id.to_string());
            format!("{tenant}|{}:{resource}|{}:{record_id}", resource.len(), record_id.len())
        }
        fn lock_from_row(row: sea_orm::QueryResult) -> Result<RealtimeLockLease, DbErr> {
            Ok(RealtimeLockLease {
                lease_token: row.try_get("", "lease_token")?, actor_id: row.try_get("", "actor_id")?,
                tenant_id: row.try_get("", "tenant_id")?, resource: row.try_get("", "resource")?,
                record_id: row.try_get("", "record_id")?, acquired_at: row.try_get("", "acquired_at")?,
                expires_at: row.try_get("", "expires_at")?,
            })
        }
    }
}
