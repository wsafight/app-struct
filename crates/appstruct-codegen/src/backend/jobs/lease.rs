use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn methods(lease_seconds: i64) -> TokenStream {
    let interval_ms = (lease_seconds * 1000 / 3).max(100);
    quote! {
        async fn handle_with_lease(&self, job: &Job) -> Result<Result<(), JobHandlerError>, JobError> {
            let heartbeat = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(#interval_ms as u64)).await;
                    let result = self.database.execute_raw(Statement::from_sql_and_values(
                        DbBackend::Postgres,
                        "UPDATE \"_appstruct_jobs\" SET locked_until = clock_timestamp() + ($4 * INTERVAL '1 second') WHERE id = $1 AND status = 'running' AND locked_by = $2 AND attempts = $3 AND locked_until > clock_timestamp()",
                        [job.id.into(), self.worker_id.clone().into(), job.attempts.into(), #lease_seconds.into()],
                    )).await?;
                    if result.rows_affected() != 1 { return Err(JobError::LeaseLost); }
                }
            };
            tokio::select! {
                result = self.handler.handle(job) => Ok(result),
                result = heartbeat => result,
            }
        }
    }
}
