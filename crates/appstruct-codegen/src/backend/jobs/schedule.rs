use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn definitions(ir: &AppIr) -> TokenStream {
    let schedules = ir.jobs.schedules.iter().map(|schedule| {
        let name = &schedule.name;
        let cron = &schedule.cron;
        let interval = i64::try_from(schedule.interval_seconds).unwrap_or(86_400);
        let queue = &schedule.queue;
        let kind = &schedule.kind;
        let payload = &schedule.payload;
        quote! {
            ScheduleConfig {
                name: #name, cron: #cron, interval_seconds: #interval,
                queue: #queue, kind: #kind, payload: #payload,
            }
        }
    });
    quote! {
        #[derive(Clone, Copy)]
        struct ScheduleConfig {
            name: &'static str,
            cron: &'static str,
            interval_seconds: i64,
            queue: &'static str,
            kind: &'static str,
            payload: &'static str,
        }
        fn schedule_configs() -> &'static [ScheduleConfig] { &[#(#schedules),*] }
    }
}

pub(super) fn persistence() -> TokenStream {
    quote! {
        async fn ensure_schedules(database: &DatabaseConnection) -> Result<(), JobError> {
            for schedule in schedule_configs() {
                database.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "INSERT INTO \"_appstruct_job_schedules\" (id, name, cron, interval_seconds, queue, kind, payload, enabled, next_run_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (name) DO UPDATE SET cron = EXCLUDED.cron, interval_seconds = EXCLUDED.interval_seconds, queue = EXCLUDED.queue, kind = EXCLUDED.kind, payload = EXCLUDED.payload",
                    [
                        uuid::Uuid::now_v7().into(), schedule.name.to_owned().into(),
                        schedule.cron.to_owned().into(), schedule.interval_seconds.into(),
                        schedule.queue.to_owned().into(), schedule.kind.to_owned().into(),
                        serde_json::from_str::<serde_json::Value>(schedule.payload)
                            .map_err(|error| JobError::Serialization(error.to_string()))?.into(),
                    ],
                )).await?;
            }
            Ok(())
        }

        async fn schedule_due(database: &DatabaseConnection) -> Result<(), JobError> {
            for schedule in schedule_configs() {
                let row = database.query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "WITH due AS (SELECT id, next_run_at FROM \"_appstruct_job_schedules\" WHERE name = $1 AND enabled AND next_run_at <= CURRENT_TIMESTAMP FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE \"_appstruct_job_schedules\" AS schedule SET next_run_at = schedule.next_run_at + (schedule.interval_seconds * INTERVAL '1 second'), last_run_at = CURRENT_TIMESTAMP FROM due WHERE schedule.id = due.id RETURNING due.next_run_at",
                    [schedule.name.to_owned().into()],
                )).await?;
                let Some(row) = row else { continue };
                let run_at: chrono::DateTime<chrono::Utc> = row.try_get("", "next_run_at")?;
                let idempotency = format!("schedule:{}:{}", schedule.name, run_at.timestamp());
                enqueue(
                    database, schedule.queue, schedule.kind,
                    &serde_json::from_str::<serde_json::Value>(schedule.payload)
                        .map_err(|error| JobError::Serialization(error.to_string()))?,
                    Some(&idempotency), Some(run_at), None,
                ).await?;
            }
            Ok(())
        }
    }
}
