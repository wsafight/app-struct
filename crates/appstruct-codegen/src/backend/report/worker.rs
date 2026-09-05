use proc_macro2::TokenStream;
use quote::quote;

#[allow(clippy::too_many_lines)]
pub(super) fn source() -> TokenStream {
    quote! {
        #[derive(Clone)]
        pub(crate) struct ReportJobHandler {
            database: sea_orm::DatabaseConnection,
            file: crate::FileState,
        }

        impl ReportJobHandler {
            pub(crate) fn new(
                database: sea_orm::DatabaseConnection, file: crate::FileState,
            ) -> Self { Self { database, file } }

            async fn render(&self, job: &crate::Job) -> Result<(), String> {
                let payload: ReportJobPayload = serde_json::from_value(job.payload.clone())
                    .map_err(|_| "REPORT_JOB_PAYLOAD_INVALID".to_owned())?;
                let work = load_report_work(&self.database, job, payload.run_id).await?;
                if matches!(work.stage.as_str(), "succeeded" | "cancelled") { return Ok(()); }
                update_report_stage(&self.database, work.id, "rendering", 10, None).await?;
                let snapshot = decrypt_snapshot(work.id, &work.snapshot_ciphertext)?;
                if sha256_hex(&snapshot) != work.snapshot_digest {
                    return Err("REPORT_SNAPSHOT_INVALID".to_owned());
                }
                let input: serde_json::Value = serde_json::from_slice(&snapshot)
                    .map_err(|_| "REPORT_SNAPSHOT_INVALID".to_owned())?;
                let content = render_capture_pdf(
                    &work.body, &input, &work.locale, &work.timezone,
                    &work.paper, &work.orientation,
                )?;
                update_report_stage(&self.database, work.id, "publishing", 80, None).await?;
                let tenant = work.tenant_id.map_or_else(|| "global".to_owned(), |id| id.to_string());
                let object_key = format!("reports/{tenant}/{}.pdf", work.id);
                let original_name = format!("{}-{}.pdf", work.template, work.id);
                let metadata = match self.file.put(
                    &object_key, &original_name, "application/pdf", &content, work.tenant_id,
                ).await {
                    Ok(metadata) => metadata,
                    Err(_) => self.file.get(&object_key, work.tenant_id).await
                        .map(|(metadata, _)| metadata)
                        .map_err(|_| "REPORT_PUBLISH_FAILED".to_owned())?,
                };
                self.database.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "UPDATE \"_appstruct_report_runs\" SET stage = 'succeeded', progress = 100, result_file_id = $2, result_object_key = $3, error_code = NULL, completed_at = CURRENT_TIMESTAMP WHERE id = $1 AND stage <> 'cancelled'",
                    [work.id.into(), metadata.id.into(), object_key.into()],
                )).await.map_err(|_| "REPORT_STATE_UPDATE_FAILED".to_owned())?;
                Ok(())
            }
        }

        #[async_trait::async_trait]
        impl crate::JobHandler for ReportJobHandler {
            async fn handle(&self, job: &crate::Job) -> Result<(), crate::JobHandlerError> {
                if job.kind == "report.cleanup" {
                    return cleanup_expired_reports(&self.database, &self.file)
                        .await.map_err(crate::JobHandlerError);
                }
                let result = self.render(job).await;
                if let Err(code) = &result {
                    if let Ok(payload) = serde_json::from_value::<ReportJobPayload>(job.payload.clone()) {
                        let stage = if job.attempts >= job.max_attempts { "failed" } else { "queued" };
                        let _ = update_report_stage(&self.database, payload.run_id, stage, 0, Some(code)).await;
                    }
                }
                result.map_err(crate::JobHandlerError)
            }
        }

        async fn cleanup_expired_reports(
            database: &sea_orm::DatabaseConnection, file: &crate::FileState,
        ) -> Result<(), String> {
            let rows = database.query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, tenant_id, result_object_key FROM \"_appstruct_report_runs\" WHERE expires_at <= CURRENT_TIMESTAMP AND stage IN ('succeeded', 'failed', 'cancelled') ORDER BY expires_at, id LIMIT 100".to_owned(),
            )).await.map_err(|_| "REPORT_CLEANUP_LOAD_FAILED".to_owned())?;
            for row in rows {
                let id: uuid::Uuid = row.try_get("", "id")
                    .map_err(|_| "REPORT_CLEANUP_ROW_INVALID".to_owned())?;
                let tenant_id: Option<uuid::Uuid> = row.try_get("", "tenant_id")
                    .map_err(|_| "REPORT_CLEANUP_ROW_INVALID".to_owned())?;
                let object_key: Option<String> = row.try_get("", "result_object_key")
                    .map_err(|_| "REPORT_CLEANUP_ROW_INVALID".to_owned())?;
                if let Some(object_key) = object_key {
                    file.delete(&object_key, tenant_id).await
                        .map_err(|_| "REPORT_CLEANUP_FILE_FAILED".to_owned())?;
                }
                database.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "DELETE FROM \"_appstruct_report_runs\" WHERE id = $1 AND expires_at <= CURRENT_TIMESTAMP AND stage IN ('succeeded', 'failed', 'cancelled')",
                    [id.into()],
                )).await.map_err(|_| "REPORT_CLEANUP_DELETE_FAILED".to_owned())?;
            }
            Ok(())
        }

        struct ReportWork {
            id: uuid::Uuid,
            tenant_id: Option<uuid::Uuid>,
            template: String,
            body: String,
            snapshot_ciphertext: String,
            snapshot_digest: String,
            locale: String,
            timezone: String,
            paper: String,
            orientation: String,
            stage: String,
        }

        async fn load_report_work<C: ConnectionTrait>(
            database: &C, job: &crate::Job, run_id: uuid::Uuid,
        ) -> Result<ReportWork, String> {
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                concat!(
                    "SELECT r.id, r.tenant_id, t.name AS template, t.body, r.snapshot_ciphertext, r.snapshot_digest, r.locale, r.timezone, r.paper::text AS paper, r.orientation::text AS orientation, r.stage::text AS stage ",
                    "FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_report_templates\" t ON t.id = r.template_id WHERE r.id = $1 AND r.execution_job_id = $2 AND r.tenant_id IS NOT DISTINCT FROM $3"
                ),
                [run_id.into(), job.id.into(), job.tenant_id.into()],
            )).await.map_err(|_| "REPORT_RUN_LOAD_FAILED".to_owned())?
                .ok_or_else(|| "REPORT_RUN_NOT_FOUND".to_owned())?;
            Ok(ReportWork {
                id: row.try_get("", "id").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                tenant_id: row.try_get("", "tenant_id").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                template: row.try_get("", "template").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                body: row.try_get("", "body").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                snapshot_ciphertext: row.try_get("", "snapshot_ciphertext").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                snapshot_digest: row.try_get("", "snapshot_digest").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                locale: row.try_get("", "locale").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                timezone: row.try_get("", "timezone").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                paper: row.try_get("", "paper").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                orientation: row.try_get("", "orientation").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                stage: row.try_get("", "stage").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
            })
        }

        async fn update_report_stage<C: ConnectionTrait>(
            database: &C, id: uuid::Uuid, stage: &str, progress: i32, error_code: Option<&str>,
        ) -> Result<(), String> {
            database.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_report_runs\" SET stage = $2, progress = $3, error_code = $4, completed_at = CASE WHEN $2 IN ('failed', 'cancelled') THEN CURRENT_TIMESTAMP ELSE completed_at END WHERE id = $1 AND stage <> 'cancelled'",
                [id.into(), stage.to_owned().into(), progress.into(), error_code.map(str::to_owned).into()],
            )).await.map_err(|_| "REPORT_STATE_UPDATE_FAILED".to_owned())?;
            Ok(())
        }
    }
}
