use proc_macro2::TokenStream;
use quote::quote;

#[allow(clippy::too_many_lines)]
pub(super) fn source(renderer: appstruct_ir::ReportRendererIr) -> TokenStream {
    let adapter = (renderer == appstruct_ir::ReportRendererIr::Chromium)
        .then(|| quote! { #[cfg_attr(not(unix), allow(dead_code, unused_imports))] mod adapter; });
    let render = match renderer {
        appstruct_ir::ReportRendererIr::Capture => {
            quote! { render_capture_pdf(work.id, &work.body, &input, &work.locale, &work.timezone, &work.paper, &work.orientation) }
        }
        appstruct_ir::ReportRendererIr::Chromium => quote! { adapter::render(&work, &input).await },
    };
    let lifecycle = super::lifecycle::source();
    quote! {
        #adapter
        #lifecycle
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
                ensure_render_active(&self.database, job, work.id).await?;
                let configured = REPORT_TEMPLATES.iter().any(|template| template.name == work.template
                    && i32::try_from(template.version).ok() == Some(work.template_version)
                    && template.artifact_digest == work.artifact_digest && template.body == work.body);
                if !configured || work.renderer_version != REPORT_RENDERER_VERSION { return Err("REPORT_INVALID_TEMPLATE_ARTIFACT".into()); }
                update_report_stage(&self.database, work.id, "rendering", 10, None).await?;
                let snapshot = decrypt_snapshot(work.id, &work.snapshot_ciphertext)?;
                if sha256_hex(&snapshot) != work.snapshot_digest {
                    return Err("REPORT_SNAPSHOT_INVALID".to_owned());
                }
                let input: serde_json::Value = serde_json::from_slice(&snapshot)
                    .map_err(|_| "REPORT_SNAPSHOT_INVALID".to_owned())?;
                let content = tokio::select! {
                    result = async { #render } => result?,
                    error = wait_for_render_stop(&self.database, job, work.id) => return Err(error),
                };
                if content.len() as u64 > REPORT_MAX_OUTPUT_BYTES { return Err("REPORT_RESOURCE_LIMIT".into()); }
                publish_report(&self.database, &self.file, job, &work, &content).await
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
                        let stage = if job.attempts >= job.max_attempts || !retryable_job_error(&job.kind, code) { "failed" } else { "queued" };
                        let _ = update_report_failure(&self.database, job, payload.run_id, stage, code).await;
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
                let tenant = tenant_id.map_or_else(|| "global".to_owned(), |id| id.to_string());
                file.discard_unpublished(database, &format!("reports/{tenant}/{id}.pdf")).await
                    .map_err(|_| "REPORT_CLEANUP_FILE_FAILED".to_owned())?;
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
            template_version: i32,
            artifact_digest: String,
            renderer_version: String,
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
                    "SELECT r.id, r.tenant_id, t.name AS template, t.version AS template_version, t.artifact_digest, t.renderer_version, t.body, r.snapshot_ciphertext, r.snapshot_digest, r.locale, r.timezone, r.paper::text AS paper, r.orientation::text AS orientation, r.stage::text AS stage ",
                    "FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_report_templates\" t ON t.id = r.template_id WHERE r.id = $1 AND r.execution_job_id = $2 AND r.tenant_id IS NOT DISTINCT FROM $3"
                ),
                [run_id.into(), job.id.into(), job.tenant_id.into()],
            )).await.map_err(|_| "REPORT_RUN_LOAD_FAILED".to_owned())?
                .ok_or_else(|| "REPORT_RUN_NOT_FOUND".to_owned())?;
            Ok(ReportWork {
                id: row.try_get("", "id").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                tenant_id: row.try_get("", "tenant_id").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                template: row.try_get("", "template").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                template_version: row.try_get("", "template_version").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                artifact_digest: row.try_get("", "artifact_digest").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
                renderer_version: row.try_get("", "renderer_version").map_err(|_| "REPORT_RUN_INVALID".to_owned())?,
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
