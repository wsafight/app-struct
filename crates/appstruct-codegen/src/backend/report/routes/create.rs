use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source(audit: bool) -> TokenStream {
    let audit = audit.then(|| {
        quote! {
            crate::audit::record(
                &transaction, &context, "appstruct::report::run", run.id.to_string(),
                "report.create", None, Some(&run),
            ).await?;
        }
    });
    quote! {
        async fn create_run(
            State(state): State<AppState>, headers: HeaderMap, Path(name): Path<String>,
            Json(input): Json<CreateReportRun>,
        ) -> Result<(StatusCode, Json<ReportRun>), ApiError> {
            let context = state.mutation_context(&headers).await?;
            let actor_id = context.actor().ok_or(ApiError::Unauthorized)?.id;
            let template = template_config(&name).ok_or(ApiError::UnknownReportTemplate)?;
            let key = idempotency_key(&headers)?;
            let (locale, timezone, paper, orientation) = report_options(&input)?;
            let snapshot = validate_report_input(template, &input.data)?;
            let request_material = serde_json::to_vec(&serde_json::json!({
                "template": template.name, "version": template.version,
                "artifact_digest": template.artifact_digest, "data": input.data,
                "locale": locale, "timezone": timezone, "paper": paper,
                "orientation": orientation,
            })).map_err(|_| ApiError::Internal)?;
            let request_digest = sha256_hex(&request_material);
            let tenant_id = context.tenant();
            let scope = sha256_hex(format!(
                "{}:{actor_id}:{}:{}:{key}",
                tenant_id.map_or_else(|| "global".to_owned(), |tenant| tenant.to_string()),
                template.name, template.version,
            ).as_bytes());
            let run_id = uuid::Uuid::now_v7();
            let snapshot_ciphertext = encrypt_snapshot(run_id, &snapshot)?;
            let snapshot_digest = sha256_hex(&snapshot);
            let transaction = state.database.begin().await?;
            let template_id = ensure_template(&transaction, template).await?;
            let inserted = transaction.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_report_runs\" (id, template_id, template_version, tenant_id, actor_id, idempotency_scope, idempotency_key, request_digest, snapshot_ciphertext, snapshot_digest, snapshot_size, locale, timezone, paper, orientation, stage, progress, created_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'queued', 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + ($16 * INTERVAL '1 day')) ON CONFLICT (idempotency_scope) DO NOTHING RETURNING id",
                [
                    run_id.into(), template_id.into(), i32::try_from(template.version).unwrap_or(i32::MAX).into(),
                    tenant_id.into(), actor_id.into(), scope.clone().into(), key.to_owned().into(),
                    request_digest.clone().into(), snapshot_ciphertext.into(), snapshot_digest.into(),
                    i64::try_from(snapshot.len()).unwrap_or(i64::MAX).into(), locale.clone().into(),
                    timezone.clone().into(), paper.clone().into(), orientation.clone().into(),
                    i64::from(REPORT_RETENTION_DAYS).into(),
                ],
            )).await?;
            if inserted.is_none() {
                let existing = load_run_by_scope(&transaction, &scope).await?;
                let digest = transaction.query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT request_digest FROM \"_appstruct_report_runs\" WHERE idempotency_scope = $1",
                    [scope.into()],
                )).await?.ok_or(ApiError::Internal)?.try_get::<String>("", "request_digest")?;
                if digest != request_digest { return Err(ApiError::ReportIdempotencyConflict); }
                transaction.commit().await?;
                return Ok((StatusCode::OK, Json(existing)));
            }
            let receipt = crate::jobs::enqueue(
                &transaction, REPORT_QUEUE, "report.render", &ReportJobPayload { run_id },
                Some(&format!("report:{scope}")), None, tenant_id,
            ).await.map_err(|_| ApiError::Internal)?;
            transaction.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE \"_appstruct_report_runs\" SET execution_job_id = $2 WHERE id = $1",
                [run_id.into(), receipt.id.into()],
            )).await?;
            let run = load_run(&transaction, run_id, tenant_id).await?;
            #audit
            transaction.commit().await?;
            Ok((StatusCode::ACCEPTED, Json(run)))
        }

        async fn ensure_template<C: ConnectionTrait>(
            database: &C, template: ReportTemplateConfig,
        ) -> Result<uuid::Uuid, ApiError> {
            let id = uuid::Uuid::now_v7();
            let schema: serde_json::Value = serde_json::from_str(template.input_schema)
                .map_err(|_| ApiError::ReportConfiguration)?;
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_report_templates\" (id, name, version, document_type, body, artifact_digest, input_schema, data_schema_version, renderer_version, created_at) VALUES ($1, $2, $3, 'pdf', $4, $5, $6, $7, $8, CURRENT_TIMESTAMP) ON CONFLICT (name, version) DO UPDATE SET name = EXCLUDED.name WHERE \"_appstruct_report_templates\".artifact_digest = EXCLUDED.artifact_digest AND \"_appstruct_report_templates\".input_schema = EXCLUDED.input_schema AND \"_appstruct_report_templates\".data_schema_version = EXCLUDED.data_schema_version AND \"_appstruct_report_templates\".renderer_version = EXCLUDED.renderer_version RETURNING id",
                [
                    id.into(), template.name.to_owned().into(),
                    i32::try_from(template.version).unwrap_or(i32::MAX).into(),
                    template.body.to_owned().into(), template.artifact_digest.to_owned().into(),
                    schema.into(), i32::try_from(template.data_schema_version).unwrap_or(i32::MAX).into(),
                    REPORT_RENDERER_VERSION.to_owned().into(),
                ],
            )).await?.ok_or(ApiError::ReportTemplateMismatch)?;
            Ok(row.try_get("", "id")?)
        }
    }
}
