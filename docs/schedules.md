# Interval Schedules

The Jobs module supports recurring interval schedules. Despite the `cron` field name, the accepted
syntax is intentionally limited to `@every Ns`, `@every Nm`, `@every Nh`, or `*/N * * * *`.
Calendar expressions such as `0 9 * * *` are rejected by the compiler. Use an explicit Job or an
external scheduler when execution must follow wall-clock calendar time.

```yaml
modules:
  jobs:
    enabled: true
    schedules:
      cleanup:
        cron: "@every 15m"
        queue: default
        kind: maintenance.cleanup
        payload: '{"scope":"expired"}'
```

At startup, each backend reconciles the declared schedule set into
`_appstruct_job_schedules`. Current definitions are inserted or re-enabled; changed definitions
restart from the current time; rows no longer present in the App Spec are disabled. Deploy the
matching migration before starting a release that first enables Jobs.

Schedules use fire-once, skip-backlog semantics. If a process is unavailable for several periods,
the next worker iteration enqueues one Job and computes the next run from the current database time.
It does not replay every missed interval. Advancing `next_run_at` and inserting the Job share one
PostgreSQL transaction, and the generated idempotency key prevents duplicate insertion for a claimed
occurrence.

Multiple API replicas may run the scheduler against one PostgreSQL database. Row locking with
`FOR UPDATE SKIP LOCKED` ensures only one replica claims a due definition. Monitor queued/dead Jobs
through `/admin/jobs`; schedule definitions themselves currently have no Admin editor.
