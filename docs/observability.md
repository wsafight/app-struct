# Metrics And Database Workloads

Every generated backend serves Prometheus text at `/metrics`. Counters and histograms are
process-local and reset on restart. Scrape each replica; the endpoint does not query PostgreSQL.
Keep it on the internal network. The production Web proxy does not expose it.

| Metric | Meaning |
| --- | --- |
| `appstruct_health_ready` | Application lifecycle readiness; use `/health/ready` for a database ping |
| `appstruct_http_request_duration_seconds` | Histogram of time to response headers |
| `appstruct_http_in_flight` | Requests waiting for response headers |
| `appstruct_http_dropped_observations_total` | Observations omitted after reaching label limits |
| `appstruct_job_duration_seconds` | Histogram of claimed attempt duration, including persistence |
| `appstruct_jobs_in_flight` | Active job attempts |
| `appstruct_job_retries_total` | Claimed attempts with attempt number greater than one |

HTTP labels are `route` (Axum matched template or `unmatched`), `method` (seven standard methods
or `OTHER`) and `status_class` (`1xx` through `5xx`). Health probes and metric scrapes are excluded.
Query strings, record IDs and tenant IDs are never labels. The registry admits at most 512 HTTP
label sets and bounds route length to 256 bytes. Each histogram emits ten finite buckets plus
`+Inf`, sum and count. Watch the dropped counter when adding a large number of routes.

Job kinds collapse to `mail`, `report`, `report_cleanup` or `custom`. Outcomes are `succeeded`,
`failed`, `cancelled`, `lease_lost`, `database_error` or `interrupted`; these are attempt outcomes,
not persisted queue depth or final task status. Dropping an active attempt decrements the gauge
and records `interrupted`. HTTP duration excludes streaming response bodies, including SSE.

Example PromQL:

```promql
histogram_quantile(0.95, sum by (le, route) (rate(appstruct_http_request_duration_seconds_bucket[5m])))
sum(rate(appstruct_http_request_duration_seconds_count{status_class="5xx"}[5m]))
sum(rate(appstruct_job_duration_seconds_count{outcome="lease_lost"}[5m]))
```

## PostgreSQL API Benchmark

Use a disposable PostgreSQL database whose name contains `test` or `e2e`. The runner **resets its
public schema**, generates the tenant/audit fixture, migrates it and starts a local backend.

```bash
APPSTRUCT_E2E_DATABASE_URL=postgresql://localhost/appstruct_benchmark_test \
  bash scripts/run-postgres-benchmark.sh
```

The default dataset has 25,000 rows per tenant and two tenants. The runner checks field access and
tenant isolation, then measures offset lists, cursor lists, aggregate count, individual reads and
audited create/update/soft-delete journeys at 1, 8 and 24 concurrent clients. Each phase warms up
five times and runs 200 operations. A final 15-second mixed phase combines reads, cursor lists,
counts and audited writes at the highest concurrency. CRUD latency is for the entire three-request
journey. Responses are validated, so an authorization error or incorrect row count fails even when
latency is low. It also verifies that varying resource IDs and unmatched URLs do not create metric
label sets.

Results are written to `output/benchmarks/postgres-api.json` with p50/p95/p99/max latency,
throughput, error rates, runtime/host details and workload configuration. A sibling `.prom` file
contains the metrics snapshot. PostgreSQL CI uploads both. Errors fail the run. Release-mode p95
budgets are 500 ms for query/read phases, 750 ms for mixed sustained load and 1,000 ms for the
three-request audited CRUD journey; debug-mode defaults are twice those values.

Overrides: `APPSTRUCT_BENCH_ROWS` (per tenant, at most 1,000,000), `APPSTRUCT_BENCH_ITERATIONS`
(at most 10,000), comma-separated `APPSTRUCT_BENCH_CONCURRENCIES` (each at most 64),
`APPSTRUCT_BENCH_MIXED_SECONDS`, `APPSTRUCT_BENCH_P95_MS`, `APPSTRUCT_BENCH_PROFILE`
(`release` by default or `debug`) and `APPSTRUCT_BENCH_OUTPUT`. The singular
`APPSTRUCT_BENCH_CONCURRENCY` remains accepted as a one-tier shorthand. API/Web ports use the
existing `APPSTRUCT_E2E_API_PORT` and `APPSTRUCT_E2E_WEB_PORT` overrides.

This is a bounded, closed-loop regression workload against a release backend by default, not a
production capacity estimate. Compare results on the same host, PostgreSQL version, dataset and
build profile. It does not model open-loop arrivals, connection pool exhaustion, production
storage or network latency. Use deployment-representative traffic for capacity planning.
