use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn definitions(ir: &AppIr) -> TokenStream {
    let schedules = ir.jobs.schedules.iter().map(|schedule| {
        let name = &schedule.name;
        let cron = &schedule.cron;
        let interval = schedule.interval_seconds.map_or_else(
            || quote! { None },
            |seconds| {
                let seconds = i64::try_from(seconds).unwrap_or(86_400);
                quote! { Some(#seconds) }
            },
        );
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
    let report_schedule = ir.report.enabled.then(|| {
        let queue = &ir.report.queue;
        quote! {
            ScheduleConfig {
                name: "_appstruct_report_retention", cron: "@every 24h",
                interval_seconds: Some(86_400), queue: #queue,
                kind: "report.cleanup", payload: "{}",
            }
        }
    });
    quote! {
        #[derive(Clone, Copy)]
        struct ScheduleConfig {
            name: &'static str,
            cron: &'static str,
            interval_seconds: Option<i64>,
            queue: &'static str,
            kind: &'static str,
            payload: &'static str,
        }
        fn schedule_configs() -> &'static [ScheduleConfig] {
            &[#(#schedules,)* #report_schedule]
        }
    }
}

pub(super) fn persistence() -> TokenStream {
    quote! {
        fn next_schedule_run(
            schedule: &ScheduleConfig,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<chrono::DateTime<chrono::Utc>, JobError> {
            if let Some(seconds) = schedule.interval_seconds {
                return Ok(now + chrono::Duration::seconds(seconds));
            }
            let expression = format!("0 {} *", schedule.cron);
            let parsed = <cron::Schedule as std::str::FromStr>::from_str(&expression)
                .map_err(|error| JobError::InvalidInput(format!(
                    "invalid schedule `{}`: {error}", schedule.name
                )))?;
            parsed.after(&now).next().ok_or_else(|| JobError::InvalidInput(format!(
                "schedule `{}` has no future occurrence", schedule.name
            )))
        }

        async fn ensure_schedules(database: &DatabaseConnection) -> Result<(), JobError> {
            let transaction = database.begin().await?;
            let clock = transaction.query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT CURRENT_TIMESTAMP AS scheduler_now".to_owned(),
            )).await?.ok_or_else(|| JobError::Database(DbErr::Custom(
                "database clock query returned no row".to_owned()
            )))?;
            let now: chrono::DateTime<chrono::Utc> = clock.try_get("", "scheduler_now")?;
            for schedule in schedule_configs() {
                let next_run_at = if schedule.interval_seconds.is_some() {
                    now
                } else {
                    next_schedule_run(schedule, now)?
                };
                transaction.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "INSERT INTO \"_appstruct_job_schedules\" AS schedule (id, name, cron, interval_seconds, queue, kind, payload, enabled, paused, next_run_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, FALSE, $8, CURRENT_TIMESTAMP) ON CONFLICT (name) DO UPDATE SET cron = EXCLUDED.cron, interval_seconds = EXCLUDED.interval_seconds, queue = EXCLUDED.queue, kind = EXCLUDED.kind, payload = EXCLUDED.payload, enabled = TRUE, next_run_at = CASE WHEN schedule.enabled AND schedule.cron = EXCLUDED.cron AND schedule.interval_seconds IS NOT DISTINCT FROM EXCLUDED.interval_seconds THEN schedule.next_run_at ELSE EXCLUDED.next_run_at END",
                    [
                        uuid::Uuid::now_v7().into(), schedule.name.to_owned().into(),
                        schedule.cron.to_owned().into(), schedule.interval_seconds.into(),
                        schedule.queue.to_owned().into(), schedule.kind.to_owned().into(),
                        serde_json::from_str::<serde_json::Value>(schedule.payload)
                            .map_err(|error| JobError::Serialization(error.to_string()))?.into(),
                        next_run_at.into(),
                    ],
                )).await?;
            }
            transaction.execute_unprepared(
                "UPDATE \"_appstruct_job_schedules\" SET enabled = FALSE WHERE enabled"
            ).await?;
            for schedule in schedule_configs() {
                transaction.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "UPDATE \"_appstruct_job_schedules\" SET enabled = TRUE WHERE name = $1",
                    [schedule.name.to_owned().into()],
                )).await?;
            }
            transaction.commit().await?;
            Ok(())
        }

        async fn schedule_due(database: &DatabaseConnection) -> Result<(), JobError> {
            for schedule in schedule_configs() {
                let transaction = database.begin().await?;
                let row = transaction.query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT id, next_run_at, CURRENT_TIMESTAMP AS scheduler_now FROM \"_appstruct_job_schedules\" WHERE name = $1 AND enabled AND NOT paused AND next_run_at <= CURRENT_TIMESTAMP FOR UPDATE SKIP LOCKED LIMIT 1",
                    [schedule.name.to_owned().into()],
                )).await?;
                let Some(row) = row else {
                    transaction.commit().await?;
                    continue;
                };
                let schedule_id: uuid::Uuid = row.try_get("", "id")?;
                let run_at: chrono::DateTime<chrono::Utc> = row.try_get("", "next_run_at")?;
                let scheduler_now: chrono::DateTime<chrono::Utc> =
                    row.try_get("", "scheduler_now")?;
                let next_run_at = next_schedule_run(schedule, scheduler_now)?;
                transaction.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "UPDATE \"_appstruct_job_schedules\" SET next_run_at = $2, last_run_at = CURRENT_TIMESTAMP WHERE id = $1",
                    [schedule_id.into(), next_run_at.into()],
                )).await?;
                let idempotency = format!("schedule:{}:{}", schedule.name, run_at.timestamp());
                enqueue(
                    &transaction, schedule.queue, schedule.kind,
                    &serde_json::from_str::<serde_json::Value>(schedule.payload)
                        .map_err(|error| JobError::Serialization(error.to_string()))?,
                    Some(&idempotency), Some(run_at), None,
                ).await?;
                transaction.commit().await?;
            }
            Ok(())
        }
    }
}
