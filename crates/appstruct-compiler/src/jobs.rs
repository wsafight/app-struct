use crate::surface::{SurfaceJobQueue, SurfaceJobs};
use appstruct_ir::{Diagnostic, JobQueueIr, JobScheduleIr, JobsIr, SourceSpan};
use std::str::FromStr;

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
    let interval_seconds = schedule_interval(&schedule.cron.value);
    if interval_seconds.is_none() && calendar_schedule(&schedule.cron.value).is_none() {
        diagnostics.push(Diagnostic::error(
            "AS3057",
            "schedule must be `@every <number>[s|m|h]` or a valid five-field Cron expression",
            schedule.cron.span.clone(),
        ));
    }
    JobScheduleIr {
        name: schedule.name.value.clone(),
        cron: schedule.cron.value.clone(),
        interval_seconds,
        queue: schedule.queue.value.clone(),
        kind: schedule.kind.value.clone(),
        payload,
    }
}

fn schedule_interval(value: &str) -> Option<u64> {
    let value = value.strip_prefix("@every ")?;
    let (amount, multiplier) = match value.as_bytes().last().copied()? {
        b's' => (&value[..value.len() - 1], 1),
        b'm' => (&value[..value.len() - 1], 60),
        b'h' => (&value[..value.len() - 1], 3_600),
        _ => return None,
    };
    amount
        .parse::<u64>()
        .ok()
        .and_then(|amount| amount.checked_mul(multiplier))
        .filter(|seconds| (1..=86_400).contains(seconds))
}

fn calendar_schedule(value: &str) -> Option<cron::Schedule> {
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return None;
    }
    cron::Schedule::from_str(&format!("0 {value} *")).ok()
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
