use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

mod create;
mod read;

#[allow(clippy::too_many_lines)]
pub(super) fn source(ir: &AppIr) -> TokenStream {
    let create = create::source(ir.audit.enabled);
    let read = read::source(ir.audit.enabled);
    quote! {
        pub fn router() -> Router<AppState> {
            Router::new()
                .route("/api/reports/templates", get(list_templates))
                .route("/api/reports/templates/{name}/runs", post(create_run))
                .route("/api/reports/runs", get(list_runs))
                .route("/api/reports/runs/{id}", get(get_run))
                .route("/api/reports/runs/{id}/cancel", post(cancel_run))
                .route("/api/reports/runs/{id}/download", get(download_run))
        }

        async fn list_templates(
            State(state): State<AppState>, headers: HeaderMap,
        ) -> Result<Json<Vec<ReportTemplate>>, ApiError> {
            let context = state.context(&headers).await?;
            context.actor().ok_or(ApiError::Unauthorized)?;
            let templates = REPORT_TEMPLATES.iter().map(|template| ReportTemplate {
                name: template.name.to_owned(), version: template.version,
                document_type: "pdf".to_owned(),
                artifact_digest: template.artifact_digest.to_owned(),
                input_schema: serde_json::from_str(template.input_schema)
                    .expect("compiler validated report JSON schema"),
                data_schema_version: template.data_schema_version,
                renderer_version: REPORT_RENDERER_VERSION.to_owned(),
            }).collect();
            Ok(Json(templates))
        }

        fn report_options(input: &CreateReportRun) -> Result<(String, String, String, String), ApiError> {
            let locale = input.locale.as_deref().unwrap_or("en-US");
            let timezone = input.timezone.as_deref().unwrap_or("UTC");
            let paper = input.paper.as_deref().unwrap_or("a4");
            let orientation = input.orientation.as_deref().unwrap_or("portrait");
            if !matches!(locale, "en-US" | "zh-CN") {
                return Err(ApiError::InvalidReportInput("locale must be en-US or zh-CN".to_owned()));
            }
            if !matches!(timezone, "UTC" | "Asia/Shanghai") {
                return Err(ApiError::InvalidReportInput(
                    "timezone must be UTC or Asia/Shanghai".to_owned(),
                ));
            }
            if !matches!(paper, "a4" | "letter") {
                return Err(ApiError::InvalidReportInput("paper must be a4 or letter".to_owned()));
            }
            if !matches!(orientation, "portrait" | "landscape") {
                return Err(ApiError::InvalidReportInput(
                    "orientation must be portrait or landscape".to_owned(),
                ));
            }
            Ok((locale.to_owned(), timezone.to_owned(), paper.to_owned(), orientation.to_owned()))
        }

        fn validate_report_input(
            template: ReportTemplateConfig, input: &serde_json::Value,
        ) -> Result<Vec<u8>, ApiError> {
            let bytes = serde_json::to_vec(input).map_err(|_| ApiError::InvalidReportInput(
                "report data is not valid JSON".to_owned(),
            ))?;
            if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REPORT_MAX_INPUT_BYTES {
                return Err(ApiError::InvalidReportInput(format!(
                    "report data exceeds the {REPORT_MAX_INPUT_BYTES} byte limit"
                )));
            }
            let schema = serde_json::from_str(template.input_schema)
                .map_err(|_| ApiError::ReportConfiguration)?;
            let validator = jsonschema::validator_for(&schema)
                .map_err(|_| ApiError::ReportConfiguration)?;
            if let Err(error) = validator.validate(input) {
                return Err(ApiError::InvalidReportInput(
                    error.to_string().chars().take(500).collect(),
                ));
            }
            Ok(bytes)
        }

        fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
            let key = headers.get("Idempotency-Key")
                .ok_or(ApiError::ReportIdempotencyRequired)?
                .to_str().map_err(|_| ApiError::ReportIdempotencyRequired)?;
            if key.is_empty() || key.len() > 200 || key.chars().any(char::is_control) {
                return Err(ApiError::ReportIdempotencyRequired);
            }
            Ok(key)
        }

        fn ensure_run_access(context: &RequestContext<'_>, run: &ReportRun) -> Result<(), ApiError> {
            let actor = context.actor().ok_or(ApiError::Unauthorized)?;
            if run.actor_id == Some(actor.id) || can_read_all_reports(context) {
                Ok(())
            } else {
                Err(ApiError::Forbidden)
            }
        }

        #create
        #read

        fn run_from_row(row: sea_orm::QueryResult) -> Result<ReportRun, sea_orm::DbErr> {
            Ok(ReportRun {
                id: row.try_get("", "id")?, execution_job_id: row.try_get("", "execution_job_id")?,
                template: row.try_get("", "template")?,
                template_version: u32::try_from(row.try_get::<i32>("", "template_version")?)
                    .unwrap_or_default(),
                tenant_id: row.try_get("", "tenant_id")?, actor_id: row.try_get("", "actor_id")?,
                stage: row.try_get("", "stage")?,
                progress: u32::try_from(row.try_get::<i32>("", "progress")?).unwrap_or_default(),
                locale: row.try_get("", "locale")?, timezone: row.try_get("", "timezone")?,
                paper: row.try_get("", "paper")?, orientation: row.try_get("", "orientation")?,
                result_file_id: row.try_get("", "result_file_id")?,
                error_code: row.try_get("", "error_code")?, created_at: row.try_get("", "created_at")?,
                completed_at: row.try_get("", "completed_at")?, expires_at: row.try_get("", "expires_at")?,
            })
        }
    }
}
