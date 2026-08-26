use super::value::{ensure_known_keys, expect_bool, expect_mapping, expect_u64};
use super::{Located, SurfaceJobQueue, SurfaceJobs};
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
        &["enabled", "poll_interval_ms", "lease_seconds", "queues"],
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
        span: Some(entry.value.span.clone()),
    })
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
