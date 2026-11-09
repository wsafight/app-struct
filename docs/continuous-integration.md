# Repository quality gates

The `CI` workflow runs on pull requests, pushes to `main`, every Monday at 02:23 UTC,
and manual dispatches. Tagged releases reuse the same `quality.yml` workflow in full mode
before building the five platform archives or creating a draft Release.

| Gate | Pull request | Main, weekly, manual, release |
| --- | --- | --- |
| Rust format, Clippy, workspace tests, dependency policy and shell cleanup | Yes | Yes |
| Generated minimal/dashboard/saas Web audit, format, lint, types, tests and build | All three | All three |
| Documentation types, build, links and desktop/mobile browser tests | Yes | Yes |
| SaaS product desktop/mobile browser journeys against the generated example Web | Yes | Yes |
| PostgreSQL migration, introspection and authorization contracts | Yes | Yes |
| PostgreSQL query and combined operations/browser regressions | Yes | Yes |
| Dashboard browser, Tenant, Audit, Mail, Jobs/Webhooks/Realtime, File, SaaS, bulk and Chromium reports | — | Yes |
| Docker first deployment and renderer isolation | — | Yes |
| Windows x64 native build, archive and installer | — | Yes |
| Crate package verification and developer/API benchmarks | — | Yes |

Every PostgreSQL matrix job gets its own PostgreSQL 17 service and dedicated test database.
The suites may reset that database's `public` schema. They do not use deployment credentials
or an existing application database. Browser suites install the pinned Chromium version before
starting. CI allows up to ten minutes for a cold backend startup; local scripts keep their existing
defaults unless `APPSTRUCT_E2E_STARTUP_TIMEOUT` is set.

Cargo registry downloads and the pnpm store are cached. Linux CLI and Rust checks additionally
cache debug dependency artifacts keyed by OS, architecture, toolchain and Cargo locks/manifests;
Cargo still validates current source inputs. All three templates verify and reuse the Linux CLI
artifact built by the same workflow instead of compiling separately. Generated projects and their
backend targets remain disposable. Local scripts still build by default, unless an executable
`APPSTRUCT_TEMPLATE_CLI` is explicitly provided. Compiler/generator tests use `APPSTRUCT_PERF_BUDGET_MULTIPLIER=2`
on shared runners; the full developer/API benchmarks retain the budgets in their scripts.

## Diagnosing failures

`PostgreSQL (product)` runs database-backed product regressions on every PR and full gate,
covering credits, signed payment events, attachments, concurrent seats, MFA, approvals and a local
AI stub. `scripts/run-product-e2e.sh` supplies disposable test settings and enables
`APPSTRUCT_REQUIRE_PRODUCT_TESTS=1`; missing required settings fail instead of silently skipping.
Local runs require a dedicated database containing `test` or `e2e` in its name, not real Stripe
credentials or a model service.

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

## Platform acceptance before release

Select a branch and manually run `CI` in Actions for the full gates. No tag or Release is created.
Linux `Docker deployment` runs `scripts/run-deployment-e2e.sh --docker`; `Renderer isolation`
checks container isolation. `Windows installer` builds the current CLI on Windows 2022 x64,
reads the workspace version, packages a local archive and verifies installation to a path with
spaces, replacement, corrupt-archive rejection and original-binary preservation. It needs no
published archive. Windows archives are retained for seven days; build/installer failures remain
in the job log.

Status on 2026-10-27: macOS native installation, builds, browser journeys and database restoration
passed; see [delivery optimization](optimization-progress.md). Linux container and Windows
installer jobs have not been executed for this change. Gate configuration is not a passing result;
use the Actions run for the relevant commit after execution.
