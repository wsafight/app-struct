use crate::surface::{SurfaceJobQueue, SurfaceJobs};
use appstruct_ir::{Diagnostic, JobQueueIr, JobsIr, SourceSpan};

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
    let queues = jobs
        .queues
        .iter()
        .map(|queue| lower_queue(queue, diagnostics))
        .collect();
    JobsIr {
        enabled: true,
        poll_interval_ms,
        lease_seconds,
        queues,
    }
}

fn disabled() -> JobsIr {
    JobsIr {
        enabled: false,
        poll_interval_ms: 250,
        lease_seconds: 30,
        queues: Vec::new(),
    }
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
