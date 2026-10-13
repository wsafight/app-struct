# Repository quality gates

The `CI` workflow runs on pull requests, pushes to `main`, every Monday at 02:23 UTC,
and manual dispatches. Tagged releases reuse the same `quality.yml` workflow in full mode
before building the five platform archives or creating a draft Release.

| Gate | Pull request | Main, weekly, manual, release |
| --- | --- | --- |
| Rust format, Clippy, workspace tests, dependency policy and shell cleanup | Yes | Yes |
| Generated minimal/dashboard/saas Web audit, format, lint, types, tests and build | All three | All three |
| Documentation types, build, links and desktop/mobile browser tests | Yes | Yes |
| PostgreSQL migration, introspection and authorization contracts | Yes | Yes |
| PostgreSQL query and combined operations/browser regressions | Yes | Yes |
| Dashboard browser, Tenant, Audit, Mail, Jobs/Webhooks/Realtime, File, SaaS, bulk and Chromium reports | — | Yes |
| Docker first deployment and renderer isolation | — | Yes |
| Crate package verification and developer/API benchmarks | — | Yes |

Every PostgreSQL matrix job gets its own PostgreSQL 17 service and dedicated test database.
The suites may reset that database's `public` schema. They do not use deployment credentials
or an existing application database. Browser suites install the pinned Chromium version before
starting. CI allows up to ten minutes for a cold backend startup; local scripts keep their existing
defaults unless `APPSTRUCT_E2E_STARTUP_TIMEOUT` is set.

Only Cargo registry downloads and the pnpm store are cached. Generated projects and backend
targets remain disposable. Compiler/generator tests use `APPSTRUCT_PERF_BUDGET_MULTIPLIER=2`
on shared runners; the full developer/API benchmarks retain the budgets in their scripts.

## Diagnosing failures

PostgreSQL and deployment jobs upload process logs and available screenshots/traces for seven
days. Benchmark JSON, Prometheus snapshots and process logs are retained for thirty days.
Documentation browser failures retain their screenshots for seven days. The Actions job log
contains build output even when a suite fails before starting a process.

The E2E cleanup copies top-level regular `*.log` files before deleting its temporary project;
it does not archive environment files, session cookies, reports or the whole project. Docker
deployment also captures Compose logs before stopping containers. To retain the same process
logs during a local run:

```bash
APPSTRUCT_E2E_LOG_DIR="$PWD/output/e2e-logs" \
APPSTRUCT_E2E_DATABASE_URL=postgresql://user@localhost/appstruct_operations_e2e \
  bash scripts/run-operations-e2e.sh
```

Use a disposable test database for that command. `scripts/test-e2e-cleanup.sh` verifies log
retention, successful and failed exit statuses, symlink exclusion and temporary target cleanup
without connecting to PostgreSQL.

## Release and branch protection

The release workflow waits for all full quality gates before building archives. Archive checksums
and native installer tests still run on each supported platform; publishing remains a separate
maintainer action. The Docker deployment gate runs `deploy verify` after migrations and the
production build, so the checked manifest includes the migration files being delivered.

To enforce checks before merging, configure required checks in GitHub branch protection.
Committing these workflows does not change repository settings. Linux container acceptance and
the five-platform installer matrix require their actual Actions runs; local native tests do not
establish those results.
