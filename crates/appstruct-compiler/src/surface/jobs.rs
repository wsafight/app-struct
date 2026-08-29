use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_scalar_string, expect_u64,
};
use super::{Located, SurfaceJobQueue, SurfaceJobSchedule, SurfaceJobs};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceJobs, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceJobs::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("jobs") else {
        return Ok(SurfaceJobs::default());
    };
    let jobs = expect_mapping(&entry.value, "`modules.jobs`")?;
    ensure_known_keys(
        jobs,
        &[
            "enabled",
            "poll_interval_ms",
            "lease_seconds",
            "queues",
            "schedules",
        ],
        "`modules.jobs`",
    )?;
    let enabled = jobs
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.jobs.enabled`"))
        .transpose()?
        .unwrap_or(true);
    Ok(SurfaceJobs {
        enabled,
        poll_interval_ms: number(jobs.get("poll_interval_ms"), "jobs poll interval")?,
        lease_seconds: number(jobs.get("lease_seconds"), "jobs lease")?,
        queues: jobs
            .get("queues")
            .map(decode_queues)
            .transpose()?
            .unwrap_or_default(),
        schedules: jobs
            .get("schedules")
            .map(decode_schedules)
            .transpose()?
            .unwrap_or_default(),
        span: Some(entry.value.span.clone()),
    })
}

fn decode_schedules(entry: &MappingEntry) -> Result<Vec<SurfaceJobSchedule>, Diagnostic> {
    let schedules = expect_mapping(&entry.value, "`modules.jobs.schedules`")?;
    schedules
        .iter()
        .map(|(name, entry)| {
            let schedule = expect_mapping(&entry.value, "job schedule")?;
            ensure_known_keys(
                schedule,
                &["cron", "queue", "kind", "payload"],
                "job schedule",
            )?;
            let required = |key: &str, context: &str| {
                schedule
                    .get(key)
                    .map(|entry| expect_scalar_string(&entry.value, context))
                    .transpose()?
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "AS3053",
                            format!("job schedule requires `{key}`"),
                            entry.value.span.clone(),
                        )
                    })
            };
            Ok(SurfaceJobSchedule {
                name: Located {
                    value: name.clone(),
                    span: entry.key_span.clone(),
                },
                cron: required("cron", "schedule cron")?,
                queue: required("queue", "schedule queue")?,
                kind: required("kind", "schedule kind")?,
                payload: schedule
                    .get("payload")
                    .map(|entry| expect_scalar_string(&entry.value, "schedule payload"))
                    .transpose()?,
            })
        })
        .collect()
}

fn decode_queues(entry: &MappingEntry) -> Result<Vec<SurfaceJobQueue>, Diagnostic> {
    let queues = expect_mapping(&entry.value, "`modules.jobs.queues`")?;
    queues
        .iter()
        .map(|(name, entry)| {
            let queue = expect_mapping(&entry.value, "job queue")?;
            ensure_known_keys(queue, &["max_attempts", "backoff_seconds"], "job queue")?;
            Ok(SurfaceJobQueue {
                name: Located {
                    value: name.clone(),
                    span: entry.key_span.clone(),
                },
                max_attempts: number(queue.get("max_attempts"), "queue max attempts")?,
                backoff_seconds: number(queue.get("backoff_seconds"), "queue backoff")?,
            })
        })
        .collect()
}

fn number(entry: Option<&MappingEntry>, context: &str) -> Result<Option<Located<u64>>, Diagnostic> {
    entry
        .map(|entry| expect_u64(&entry.value, context))
        .transpose()
}
