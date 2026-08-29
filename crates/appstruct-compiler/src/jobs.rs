use crate::surface::{SurfaceJobQueue, SurfaceJobs};
use appstruct_ir::{Diagnostic, JobQueueIr, JobScheduleIr, JobsIr, SourceSpan};

pub(crate) fn lower_jobs(
    jobs: &SurfaceJobs,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> JobsIr {
    if !jobs.enabled {
        return disabled();
    }
    let span = jobs.span.as_ref().unwrap_or(fallback);
    if jobs.queues.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3047",
            "enabled jobs module requires at least one queue",
            span.clone(),
        ));
    }
    let poll_interval_ms = jobs.poll_interval_ms.as_ref().map_or(250, |value| {
        check_range(
            value.value,
            10,
            60_000,
            "AS3048",
            "poll_interval_ms",
            &value.span,
            diagnostics,
        )
    });
    let lease_seconds = jobs.lease_seconds.as_ref().map_or(30, |value| {
        check_range(
            value.value,
            1,
            3_600,
            "AS3049",
            "lease_seconds",
            &value.span,
            diagnostics,
        )
    });
    let queues: Vec<JobQueueIr> = jobs
        .queues
        .iter()
        .map(|queue| lower_queue(queue, diagnostics))
        .collect();
    let schedules = jobs
        .schedules
        .iter()
        .map(|schedule| lower_schedule(schedule, &queues, diagnostics))
        .collect();
    JobsIr {
        enabled: true,
        poll_interval_ms,
        lease_seconds,
        queues,
        schedules,
    }
}

fn disabled() -> JobsIr {
    JobsIr {
        enabled: false,
        poll_interval_ms: 250,
        lease_seconds: 30,
        queues: Vec::new(),
        schedules: Vec::new(),
    }
}

fn lower_schedule(
    schedule: &crate::surface::SurfaceJobSchedule,
    queues: &[JobQueueIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> JobScheduleIr {
    if !valid_name(&schedule.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS3053",
            "schedule name must use lowercase letters, digits, `_`, or `-`",
            schedule.name.span.clone(),
        ));
    }
    if !queues
        .iter()
        .any(|queue| queue.name == schedule.queue.value)
    {
        diagnostics.push(Diagnostic::error(
            "AS3054",
            format!(
                "schedule references unknown queue `{}`",
                schedule.queue.value
            ),
            schedule.queue.span.clone(),
        ));
    }
    if schedule.kind.value.trim().is_empty() || schedule.kind.value.len() > 120 {
        diagnostics.push(Diagnostic::error(
            "AS3055",
            "schedule kind must contain between 1 and 120 bytes",
            schedule.kind.span.clone(),
        ));
    }
    let payload = schedule
        .payload
        .as_ref()
        .map_or_else(|| "{}".to_owned(), |value| value.value.clone());
    if serde_json::from_str::<serde_json::Value>(&payload).is_err() {
        diagnostics.push(Diagnostic::error(
            "AS3056",
            "schedule payload must be valid JSON",
            schedule
                .payload
                .as_ref()
                .map_or(schedule.kind.span.clone(), |value| value.span.clone()),
        ));
    }
    let interval_seconds = cron_interval(&schedule.cron.value).unwrap_or_else(|| {
        diagnostics.push(Diagnostic::error(
            "AS3057",
            "schedule cron must be `@every Ns` or a five-field expression with a fixed minute interval",
            schedule.cron.span.clone(),
        ));
        60
    });
    JobScheduleIr {
        name: schedule.name.value.clone(),
        cron: schedule.cron.value.clone(),
        interval_seconds,
        queue: schedule.queue.value.clone(),
        kind: schedule.kind.value.clone(),
        payload,
    }
}

fn cron_interval(value: &str) -> Option<u64> {
    if let Some(seconds) = value
        .strip_prefix("@every ")
        .and_then(|value| value.strip_suffix('s'))
    {
        return seconds
            .parse::<u64>()
            .ok()
            .filter(|seconds| (1..=86_400).contains(seconds));
    }
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 || fields[1..].iter().any(|field| *field != "*") {
        return None;
    }
    let minute = fields[0];
    if minute == "*" {
        return Some(60);
    }
    minute
        .strip_prefix("*/")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|minutes| (1..=1_440).contains(minutes))
        .map(|minutes| minutes * 60)
}

fn lower_queue(queue: &SurfaceJobQueue, diagnostics: &mut Vec<Diagnostic>) -> JobQueueIr {
    if !valid_name(&queue.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS3050",
            "job queue name must use lowercase letters, digits, `_`, or `-`",
            queue.name.span.clone(),
        ));
    }
    let max_attempts = queue.max_attempts.as_ref().map_or(5, |value| {
        check_range(
            value.value,
            1,
            100,
            "AS3051",
            "max_attempts",
            &value.span,
            diagnostics,
        )
    });
    let backoff_seconds = queue.backoff_seconds.as_ref().map_or(2, |value| {
        check_range(
            value.value,
            1,
            3_600,
            "AS3052",
            "backoff_seconds",
            &value.span,
            diagnostics,
        )
    });
    JobQueueIr {
        name: queue.name.value.clone(),
        max_attempts: u32::try_from(max_attempts).unwrap_or(100),
        backoff_seconds,
    }
}

fn check_range(
    value: u64,
    minimum: u64,
    maximum: u64,
    code: &'static str,
    name: &str,
    span: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> u64 {
    if !(minimum..=maximum).contains(&value) {
        diagnostics.push(Diagnostic::error(
            code,
            format!("`{name}` must be between {minimum} and {maximum}"),
            span.clone(),
        ));
    }
    value.clamp(minimum, maximum)
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}
