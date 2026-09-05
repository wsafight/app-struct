use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn run_once(lease_seconds: i64) -> TokenStream {
    quote! {
        pub async fn run_once(&self) -> Result<bool, JobError> {
            let Some(job) = claim(
                &self.database, &self.worker_id, #lease_seconds, self.kind.as_deref(),
            ).await? else { return Ok(false); };
            let mut observation = crate::metrics::JobAttempt::new(&job.kind, job.attempts);
            let result = match self.handle_with_lease(&job).await {
                Ok(Ok(())) => complete(&self.database, &self.worker_id, job.id)
                    .await.map(|()| "succeeded"),
                Ok(Err(error)) => fail(&self.database, &self.worker_id, &job, &error.0)
                    .await.map(|()| match error.0.as_str() {
                        "REPORT_CANCELLED" => "cancelled",
                        "REPORT_LEASE_LOST" => "lease_lost",
                        _ => "failed",
                    }),
                Err(error) => Err(error),
            };
            observation.finish(match &result {
                Ok(outcome) => outcome,
                Err(JobError::LeaseLost) => {
                    let cancelled = self.database.query_one_raw(Statement::from_sql_and_values(
                        DbBackend::Postgres,
                        "SELECT (status = 'cancelled' AND attempts = $2) AS cancelled FROM \"_appstruct_jobs\" WHERE id = $1",
                        [job.id.into(), job.attempts.into()],
                    )).await.ok().flatten().and_then(|row| row.try_get::<bool>("", "cancelled").ok()).unwrap_or(false);
                    if cancelled { "cancelled" } else { "lease_lost" }
                },
                Err(_) => "database_error",
            });
            result.map(|_| true)
        }
    }
}
