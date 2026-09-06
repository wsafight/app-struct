pub(super) fn worker_values(poll_interval_ms: u64, lease_seconds: u64) -> (i64, u64) {
    (
        i64::try_from(lease_seconds).unwrap_or(30),
        poll_interval_ms.saturating_mul(32).min(5_000),
    )
}
