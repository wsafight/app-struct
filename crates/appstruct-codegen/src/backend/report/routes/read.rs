use proc_macro2::TokenStream;
use quote::quote;

#[allow(clippy::too_many_lines)]
pub(super) fn source(audit: bool) -> TokenStream {
    let cancel_audit = audit.then(|| {
        quote! {
            crate::audit::record(
                &transaction, &context, "appstruct::report::run", after.id.to_string(),
                "report.cancel", Some(&before), Some(&after),
            ).await?;
        }
    });
    let download_audit = audit.then(|| {
        quote! {
            crate::audit::record(
                &state.database, &context, "appstruct::report::run", run.id.to_string(),
                "report.download", Some(&run), Some(&run),
            ).await?;
        }
    });
    quote! {
        async fn list_runs(
            State(state): State<AppState>, headers: HeaderMap, Query(query): Query<ReportRunQuery>,
        ) -> Result<Json<ReportRunList>, ApiError> {
            let context = state.context(&headers).await?;
            let actor_id = context.actor().ok_or(ApiError::Unauthorized)?.id;
            let page = query.page.unwrap_or(1);
            let page_size = query.page_size.unwrap_or(25);
            if !(1..=10_000).contains(&page) || !(1..=100).contains(&page_size) {
                return Err(ApiError::InvalidQuery("invalid report pagination".to_owned()));
            }
            let all = can_read_all_reports(&context);
            let tenant_id = context.tenant();
            let count = state.database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT COUNT(*) AS total FROM \"_appstruct_report_runs\" WHERE tenant_id IS NOT DISTINCT FROM $1 AND ($2 OR actor_id = $3)",
                [tenant_id.into(), all.into(), actor_id.into()],
            )).await?.ok_or(ApiError::Internal)?.try_get::<i64>("", "total")?;
            let offset = (page - 1).checked_mul(page_size)
                .ok_or_else(|| ApiError::InvalidQuery("report pagination is too large".to_owned()))?;
            let rows = state.database.query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                concat!(
                    "SELECT r.id, r.execution_job_id, t.name AS template, r.template_version, r.tenant_id, r.actor_id, r.stage::text AS stage, r.progress, r.locale, r.timezone, r.paper::text AS paper, r.orientation::text AS orientation, r.result_file_id, r.error_code, r.created_at, r.completed_at, r.expires_at ",
                    "FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_report_templates\" t ON t.id = r.template_id WHERE r.tenant_id IS NOT DISTINCT FROM $1 AND ($2 OR r.actor_id = $3) ORDER BY r.created_at DESC, r.id DESC LIMIT $4 OFFSET $5"
                ),
                [tenant_id.into(), all.into(), actor_id.into(), i64::try_from(page_size).unwrap_or(100).into(), i64::try_from(offset).unwrap_or(i64::MAX).into()],
            )).await?;
            let data = rows.into_iter().map(run_from_row).collect::<Result<Vec<_>, _>>()?;
            Ok(Json(ReportRunList { data, meta: ReportRunListMeta {
                page, page_size, total: u64::try_from(count).unwrap_or_default(),
            }}))
        }

        async fn get_run(
            State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
        ) -> Result<Json<ReportRun>, ApiError> {
            let context = state.context(&headers).await?;
            let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
            let run = load_run(&state.database, id, context.tenant()).await?;
            ensure_run_access(&context, &run)?;
            Ok(Json(run))
        }

        async fn cancel_run(
            State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
        ) -> Result<Json<ReportRun>, ApiError> {
            let context = state.mutation_context(&headers).await?;
            let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
            let transaction = state.database.begin().await?;
            let before = load_run_locked(&transaction, id, context.tenant()).await?;
            ensure_run_access(&context, &before)?;
            if !matches!(before.stage.as_str(), "queued" | "rendering") { return Err(ApiError::ReportCancellationConflict); }
            let job_id = before.execution_job_id.ok_or(ApiError::ReportCancellationConflict)?;
            let result = transaction.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_jobs\" SET status = 'cancelled', completed_at = CURRENT_TIMESTAMP, locked_by = NULL, locked_until = NULL WHERE id = $1 AND status IN ('queued', 'running')",
                [job_id.into()],
            )).await?;
            if result.rows_affected() != 1 { return Err(ApiError::ReportCancellationConflict); }
            transaction.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_report_runs\" SET stage = 'cancelled', completed_at = CURRENT_TIMESTAMP, progress = 0 WHERE id = $1",
                [id.into()],
            )).await?;
            let after = load_run(&transaction, id, context.tenant()).await?;
            #cancel_audit
            transaction.commit().await?;
            Ok(Json(after))
        }

        async fn download_run(
            State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>,
        ) -> Result<Response, ApiError> {
            let context = state.context(&headers).await?;
            let id = uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)?;
            let run = load_run(&state.database, id, context.tenant()).await?;
            ensure_run_access(&context, &run)?;
            if run.stage != "succeeded" || run.expires_at <= chrono::Utc::now() {
                return Err(ApiError::ReportNotReady);
            }
            let object_key = state.database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT result_object_key FROM \"_appstruct_report_runs\" WHERE id = $1 AND tenant_id IS NOT DISTINCT FROM $2",
                [id.into(), context.tenant().into()],
            )).await?.ok_or(ApiError::NotFound)?.try_get::<Option<String>>("", "result_object_key")?
                .ok_or(ApiError::ReportNotReady)?;
            let (_, content) = state.file.get(&object_key, context.tenant()).await
                .map_err(|_| ApiError::ReportNotReady)?;
            #download_audit
            let filename = format!("{}-{}.pdf", run.template, run.id);
            let mut response = Response::new(Body::from(content));
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                    .map_err(|_| ApiError::Internal)?,
            );
            Ok(response)
        }

        async fn load_run<C: ConnectionTrait>(
            database: &C, id: uuid::Uuid, tenant_id: Option<uuid::Uuid>,
        ) -> Result<ReportRun, ApiError> {
            load_run_query(database, "", [id.into(), tenant_id.into()]).await
        }
        async fn load_run_locked<C: ConnectionTrait>(
            database: &C, id: uuid::Uuid, tenant_id: Option<uuid::Uuid>,
        ) -> Result<ReportRun, ApiError> {
            load_run_query(database, " FOR UPDATE", [id.into(), tenant_id.into()]).await
        }
        async fn load_run_query<C: ConnectionTrait>(
            database: &C, suffix: &str, values: [sea_orm::Value; 2],
        ) -> Result<ReportRun, ApiError> {
            let sql = format!(
                "{}{}",
                concat!(
                    "SELECT r.id, r.execution_job_id, t.name AS template, r.template_version, r.tenant_id, r.actor_id, r.stage::text AS stage, r.progress, r.locale, r.timezone, r.paper::text AS paper, r.orientation::text AS orientation, r.result_file_id, r.error_code, r.created_at, r.completed_at, r.expires_at ",
                    "FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_report_templates\" t ON t.id = r.template_id WHERE r.id = $1 AND r.tenant_id IS NOT DISTINCT FROM $2"
                ),
                suffix,
            );
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres, sql, values,
            )).await?.ok_or(ApiError::NotFound)?;
            Ok(run_from_row(row)?)
        }
        async fn load_run_by_scope<C: ConnectionTrait>(
            database: &C, scope: &str,
        ) -> Result<ReportRun, ApiError> {
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                concat!(
                    "SELECT r.id, r.execution_job_id, t.name AS template, r.template_version, r.tenant_id, r.actor_id, r.stage::text AS stage, r.progress, r.locale, r.timezone, r.paper::text AS paper, r.orientation::text AS orientation, r.result_file_id, r.error_code, r.created_at, r.completed_at, r.expires_at ",
                    "FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_report_templates\" t ON t.id = r.template_id WHERE r.idempotency_scope = $1"
                ),
                [scope.to_owned().into()],
            )).await?.ok_or(ApiError::Internal)?;
            Ok(run_from_row(row)?)
        }
    }
}
