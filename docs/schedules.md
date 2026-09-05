# Schedules

The Jobs module supports elapsed-time intervals and five-field calendar Cron expressions. Calendar
expressions are evaluated in UTC. The compiler validates the complete expression before generation.

- `@every Ns`, `@every Nm`, and `@every Nh` run from the most recent scheduler claim. The interval
  must be between one second and 24 hours.
- Five-field Cron uses `minute hour day-of-month month day-of-week`, including lists, ranges, and
  steps accepted by the generated runtime's Cron parser.

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
      weekday-digest:
        cron: "0 9 * * 1-5"
        queue: default
        kind: reports.weekday-digest
```

At startup, each backend reconciles the declared schedule set into
`_appstruct_job_schedules`. Current definitions are inserted or re-enabled; changed interval
definitions restart from the current time, changed calendar definitions advance to their next UTC
occurrence, and rows no longer present in the App Spec are disabled. Deploy the matching migration
before starting a release that first enables Jobs.

Schedules use fire-once, skip-backlog semantics. If a process is unavailable for several periods,
the next worker iteration enqueues one Job and computes the next run from the current database time.
It does not replay every missed interval. Advancing `next_run_at` and inserting the Job share one
PostgreSQL transaction, and the generated idempotency key prevents duplicate insertion for a claimed
occurrence. Calendar schedules always advance to the first future matching time.

Multiple API replicas may run the scheduler against one PostgreSQL database. Row locking with
`FOR UPDATE SKIP LOCKED` ensures only one replica claims a due definition. Monitor queued/dead Jobs
through `/admin/jobs`. Administrators can inspect definitions at `/admin/schedules`, pause or resume
active schedules, and enqueue an immediate run. Pausing is stored separately from definition
reconciliation, so a process restart does not silently resume a schedule. Definitions remain owned
by the App Spec and cannot be edited from the Admin UI.
