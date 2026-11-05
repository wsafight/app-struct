# Metrics And Database Workloads

Generated backends emit text `tracing` logs by default. Set `APPSTRUCT_LOG_FORMAT=json` for
newline-delimited structured JSON; `RUST_LOG` controls filtering in either format. HTTP spans carry
bounded `request_id` and `method` fields, and the request ID is also returned in `X-Request-Id`.
Route, status, and latency are represented by the bounded metrics below so record IDs, query
strings, and tenant IDs do not become high-cardinality log or metric fields. This version does not
embed an OTLP exporter; deployments can collect JSON logs until an optional Runtime exporter
contract is frozen.

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

## Million-row reference run

One run on 2026-11-04 used 1,000,000 rows per tenant (two tenants), a release backend, PostgreSQL 17.11 and an Apple M3 Pro, with the backend, database and load generator on the same machine. It is a single sample, not an acceptance test. The run exited non-zero because two phases exceeded the default 500 ms budget at 24 clients. There were no request errors.

| Phase | 1 client p95 | 8 clients p95 | 24 clients p95 |
| --- | --- | --- | --- |
| Offset list (page numbers, exact total) | 51 ms | 408 ms | 744 ms (over budget) |
| Aggregate count | 49 ms | 452 ms | 692 ms (over budget) |
| Cursor list | 0.8 ms | 1.8 ms | 3.7 ms |
| Individual read | 0.6 ms | 1.2 ms | 5.7 ms |
| Audited CRUD journey | 1.5 ms | 7.8 ms | 11.6 ms |

The 24-client mixed sustained phase had a p95 of 570 ms against a 750 ms budget, with 0 errors in 3,733 operations.

Offset lists return an exact `meta.total`, so every page request pays for a `COUNT(*)` over the tenant's rows. Query plans on a separate single-connection database with the same data shape showed:

- The exact count was a parallel sequential scan of about 51 ms per request. It stayed a sequential scan with a composite index on `(tenant_id, code, id)` restricted to live rows, so an index does not remove this cost.
- Deep `OFFSET` pages also cost more. At offset 500,000 the page took about 200 to 260 ms, and at offset 990,000 about 417 ms, with a cold-cache outlier of 2.6 s. With that composite index the same pages took about 78 ms and 127 ms. When deep pages matter, declare such an index with [schema indexes](schema-indexes.md); on a `tenant: true` soft-delete entity, `fields: [tenant_id, code, id]` with `where: deleted_at IS NULL` compiles to that index.

Cursor lists do not compute a total or skip rows, so they stay in single-digit milliseconds at this size. For clients that do not need page numbers or a total, such as a mobile client, use `limit` with cursors.

Page-number lists with exact totals remain the default. Treat roughly 1,000,000 rows per tenant at 24 concurrent clients on a single host as the point where they can exceed the default budget. This run did not test estimated or capped totals, so no change to the list contract was made. Per-request cost at one client (about 50 ms) is the more relevant figure for a single user.
