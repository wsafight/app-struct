use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source() -> TokenStream {
    quote! {
        pub(crate) fn retryable_job_error(kind: &str, code: &str) -> bool {
            kind != "report.render" || !matches!(code,
                "REPORT_INVALID_TEMPLATE_ARTIFACT" | "REPORT_BLOCKED_RESOURCE" | "REPORT_RESOURCE_LIMIT"
                | "REPORT_INVALID_OUTPUT" | "REPORT_TEMPLATE_RENDER_FAILED" | "REPORT_SNAPSHOT_INVALID"
                | "REPORT_CANCELLED")
        }
        async fn ensure_render_active<C: ConnectionTrait>(database: &C, job: &crate::Job, run_id: uuid::Uuid) -> Result<(), String> {
            let row = database.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "SELECT r.stage::text AS stage, (j.status = 'running' AND j.attempts = $3 AND j.locked_until > clock_timestamp()) AS owned FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_jobs\" j ON j.id = r.execution_job_id WHERE r.id = $1 AND j.id = $2 AND r.tenant_id IS NOT DISTINCT FROM $4",
                [run_id.into(), job.id.into(), job.attempts.into(), job.tenant_id.into()],
            )).await.map_err(|_| "REPORT_RUN_LOAD_FAILED")?.ok_or("REPORT_LEASE_LOST")?;
            if row.try_get::<String>("", "stage").map_err(|_| "REPORT_RUN_INVALID")? == "cancelled" { return Err("REPORT_CANCELLED".into()); }
            if !row.try_get::<bool>("", "owned").map_err(|_| "REPORT_RUN_INVALID")? { return Err("REPORT_LEASE_LOST".into()); }
            Ok(())
        }
        async fn wait_for_render_stop(database: &sea_orm::DatabaseConnection, job: &crate::Job, run_id: uuid::Uuid) -> String {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if let Err(error) = ensure_render_active(database, job, run_id).await { return error; }
            }
        }
        async fn update_report_failure<C: ConnectionTrait>(database: &C, job: &crate::Job, run_id: uuid::Uuid, stage: &str, code: &str) -> Result<(), String> {
            database.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "UPDATE \"_appstruct_report_runs\" r SET stage = $3, progress = 0, error_code = $4, completed_at = CASE WHEN $3 = 'failed' THEN CURRENT_TIMESTAMP ELSE r.completed_at END FROM \"_appstruct_jobs\" j WHERE r.id = $1 AND r.execution_job_id = j.id AND j.id = $2 AND j.attempts = $5 AND j.status = 'running' AND j.locked_until > clock_timestamp() AND r.stage NOT IN ('cancelled', 'succeeded')",
                [run_id.into(), job.id.into(), stage.to_owned().into(), code.to_owned().into(), job.attempts.into()],
            )).await.map_err(|_| "REPORT_STATE_UPDATE_FAILED")?;
            Ok(())
        }
        async fn publish_report(database: &sea_orm::DatabaseConnection, file: &crate::FileState, job: &crate::Job, work: &ReportWork, content: &[u8]) -> Result<(), String> {
            let transaction = database.begin().await.map_err(|_| "REPORT_PUBLISH_FAILED")?;
            let lock = transaction.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "SELECT r.id FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_jobs\" j ON j.id = r.execution_job_id WHERE r.id = $1 AND j.id = $2 FOR UPDATE OF r, j",
                [work.id.into(), job.id.into()],
            )).await.map_err(|_| "REPORT_PUBLISH_FAILED")?;
            if lock.is_none() { return Err("REPORT_LEASE_LOST".into()); }
            ensure_render_active(&transaction, job, work.id).await?;
            update_report_stage(&transaction, work.id, "publishing", 80, None).await?;
            let tenant = work.tenant_id.map_or_else(|| "global".to_owned(), |id| id.to_string());
            let object_key = format!("reports/{tenant}/{}.pdf", work.id);
            let original_name = format!("{}-{}.pdf", work.template, work.id);
            file.discard_unpublished(&transaction, &object_key).await.map_err(|_| "REPORT_PUBLISH_FAILED")?;
            let metadata = match file.get_with_connection(&transaction, &object_key, work.tenant_id).await {
                Ok((metadata, bytes)) if metadata.content_type == "application/pdf" && bytes.starts_with(b"%PDF-") => metadata,
                _ => file.put_with_connection(&transaction, &object_key, &original_name, "application/pdf", content, work.tenant_id).await.map_err(|_| "REPORT_PUBLISH_FAILED")?,
            };
            transaction.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "UPDATE \"_appstruct_report_runs\" SET stage = 'succeeded', progress = 100, result_file_id = $2, result_object_key = $3, error_code = NULL, completed_at = CURRENT_TIMESTAMP WHERE id = $1",
                [work.id.into(), metadata.id.into(), object_key.into()],
            )).await.map_err(|_| "REPORT_STATE_UPDATE_FAILED")?;
            transaction.commit().await.map_err(|_| "REPORT_PUBLISH_FAILED")?;
            Ok(())
        }
    }
}
