# AppStruct

[English](README.md) | [简体中文](README.zh-CN.md)

AppStruct is a configuration-driven Rust full-stack application generator. It compiles a
multi-file YAML App Spec into a typed IR, PostgreSQL migrations, an Axum/SeaORM backend,
OpenAPI, a TypeScript client, and a React/Vite application.

The repository is currently a technical preview. It is distributed from a source checkout for now;
there is no crates.io package or binary installer yet. M0-M6 are complete, including production builds,
the coordinated development server, Tenant/Audit/Mail/Jobs/File modules, the locked
`appstruct/saas@1` preset, transactional project updates, and a runnable SaaS template and example.
The SaaS preset includes an Admin operations overview and guarded Jobs retry/replay controls;
Billing remains out of scope for preset version 1.

## Quick Start

The workspace pins Rust 1.98.0. Install the CLI from this checkout:

```bash
rustup toolchain install 1.98.0 --component clippy,rustfmt
cargo build --release --locked -p appstruct-cli
export PATH="$PWD/target/release:$PATH"
appstruct --version
```

Create a minimal project backed by an existing PostgreSQL database:

```bash
appstruct new notes --template minimal
cd notes
cp .env.example .env
export DATABASE_URL=postgresql://appstruct:appstruct-dev@127.0.0.1:5432/appstruct
appstruct migrate dev --accept
appstruct doctor
appstruct dev
```

Update `DATABASE_URL` in `.env` before running `doctor`. For a Docker-managed PostgreSQL
development database, use `--template dashboard` or `--template saas` and ensure Docker Compose
is available.

`appstruct dev` starts the generated API and Vite, watches App Spec and user Rust inputs, and
stops its child processes on Ctrl-C. Managed databases default to prompting only when migrations
are required; external databases default to leaving migrations entirely to the operator.
The default URLs are `http://127.0.0.1:3000` and `http://127.0.0.1:5173`.

Generated Web applications use a pinned modern baseline: React 19, TypeScript, and Vite with
TanStack Query, TanStack Router, TanStack Table, and TanStack Form plus Zod validation. The
generated dependency lockfile is part of the project output, so installs and builds remain
repeatable across machines.

Configure development migration ownership explicitly when needed:

```yaml
database:
  provider: postgres
  dev:
    mode: managed
    migration: prompt # auto | prompt | never | unmanaged
```

`auto` creates and applies safe migrations, `prompt` asks only when work is pending, `never`
performs read-only compatibility checks and blocks stale schemas, and `unmanaged` skips all
AppStruct migration checks before starting. Production backend startup never runs migrations;
use a dedicated `migrate status` / `migrate apply` release step.

Create a review-only App Spec draft from an existing PostgreSQL schema:

```bash
appstruct db pull --schema public --output spec/imported.yaml
```

The command reads `DATABASE_URL`, never changes the database or root `includes`, and refuses to
overwrite the output. Review unsupported-shape warnings and add explicit entity access rules before
including the draft in `appstruct.yaml`. Use `--check` in CI to fail when an existing draft is stale,
or `--diff` to print the live schema changes without writing the file.

Generated list endpoints offer offset pagination with totals and primary-key cursor pagination for
large result sets. A filterable relation can traverse one hop to target fields that are also marked
filterable, while retaining target access and tenant scopes. Each resource also exposes bounded
count/sum/average/min/max and group-by queries over filterable fields. See
[Generated resource queries](docs/data-querying.md) for request, response, OpenAPI, and TypeScript
client contracts. Fields can additionally declare independent read/write access rules; unauthorized
response fields are omitted and unauthorized submitted fields are rejected by the backend.

Composite and partial indexes are declared per entity with `indexes`; see
[Schema indexes](docs/schema-indexes.md).

Named entity seed rows are declared with `seeds` and included in reviewable migrations; see
[Seed data](docs/seeding.md).

Enable tenant isolation together with Auth and mark each tenant-owned entity explicitly:

```yaml
modules:
  auth:
    enabled: true
    user_entity: User
  tenant:
    enabled: true

entities:
  Project:
    tenant: true
```

The generated Web application provides organization onboarding and switching. Generated clients
send `X-AppStruct-Tenant`; the backend validates membership and injects `tenant_id` into every
tenant-scoped CRUD query and write.

Enable Audit with Auth, choose the reader roles, and opt entities into transactional snapshots:

```yaml
modules:
  audit:
    enabled: true
    reader_roles: [admin]

entities:
  Project:
    audit: true
```

Create, update, and delete snapshots are committed in the same PostgreSQL transaction as the
business write. The generated Audit page and read-only API enforce reader roles and current-tenant
isolation.

Mail supports compile-time validated templates and `capture`, SMTP, or Resend providers. Provider
credentials remain in server environment variables, and business handlers can call the generated
`RequestContext::send_mail` capability. Capture is development-only and rejected in production.

Select the official SaaS preset to enable the implemented modules with one versioned contract:

```yaml
preset:
  name: appstruct/saas
  version: 1
```

The project must commit an `appstruct.lock` containing the preset digest and exact module versions.
User `modules` mappings override defaults recursively; scalar and list values replace their defaults.
Inspect the locked contract with `appstruct preset show` or print the effective module
configuration after project overrides with `appstruct preset show --expanded`. Admin is available
as an operations preview with Jobs recovery controls; Billing is not part of preset version 1.

## Commands

```text
appstruct new <name> --template minimal|dashboard|saas
appstruct schema
appstruct check [--deny-warnings] [--format text|json]
appstruct generate [--check]
appstruct migrate plan|dev|lint|apply|status
appstruct dev [--api-port <port>] [--web-port <port>]
appstruct build
appstruct doctor [--format text|json]
appstruct db pull [--schema <name>] [--output <project-relative-path>] [--check | --diff]
appstruct auth bootstrap-admin --email <address>
appstruct preset show [--expanded]
appstruct update
```

`migrate plan` is read-only. `migrate dev --accept` creates and optionally applies only
non-destructive online migrations. Production deployment uses `migrate status` followed by
the explicit `migrate apply` command. `migrate lint` reports destructive, locking, and unsafe
non-null changes without writing files; use `--deny-warnings` in CI to enforce operational review.

## Documentation

- [Installation](docs/installation.md)
- [Upgrading](docs/upgrading.md)
- [Deployment](docs/deployment.md)
- [Generated resource queries](docs/data-querying.md)
- [Lossless scalar values and datetime controls](docs/scalar-values.md)
- [Headless Web controllers](docs/headless-controller.md)
- [Schema indexes](docs/schema-indexes.md)
- [Seed data](docs/seeding.md)
- [Interval schedules](docs/schedules.md)
- [Signed webhooks](docs/webhooks.md)
- [Realtime events, presence, and edit leases](docs/realtime.md)
- [Entity workflows](docs/workflows.md)
- [Reports](docs/reports.md)
- [Record activity](docs/activity.md)
- [Business UI semantics](docs/business-ui-semantics.md)
- [Saved views](docs/saved-views.md)
- [Operations Admin console](docs/admin-console.md)
- [Operations Demo findings](docs/operations-demo-findings.md)
- [Aggregate line items RFC](docs/aggregate-line-items-rfc.md)
- [Production report renderer adapter RFC](docs/report-renderer-adapter-rfc.md)
- [Releasing](docs/releasing.md)
- [Next product roadmap](docs/next-product-roadmap.md)
- [Product requirements](PRODUCT.md)
- [Technical design](TECHNICAL_DESIGN.md)

The `references/` directory contains local research material and is intentionally excluded
from version control and product commits.

## Development

Run the repository quality gates with the pinned local toolchain:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run the PostgreSQL browser gate against a dedicated test database:

```bash
APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_e2e' \
  scripts/run-m5-browser-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_tenant_e2e' \
  scripts/run-m6-tenant-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_audit_e2e' \
  scripts/run-m6-audit-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_mail_e2e' \
  scripts/run-m6-mail-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_jobs_e2e' \
  scripts/run-m6-jobs-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_file_e2e' \
  scripts/run-m6-file-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_saas_e2e' \
  scripts/run-m6-saas-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_operations_e2e' \
  scripts/run-operations-e2e.sh
```

The CI and release workflows run `scripts/run-template-build.sh` on Node 24 and 25 for the minimal,
dashboard and SaaS templates. It verifies production dependency advisories, generated Web formatting, tests,
TypeScript types, and the Vite production bundle without requiring a database. Releases also run
the complete PostgreSQL E2E matrix before building binaries.

Generated backends include bounded HTTP and job metrics. A two-tenant PostgreSQL workload measures
list, cursor, aggregate, read and audited CRUD latency; see [observability](docs/observability.md).
Version-pinned installers and production Compose probes are described in
[installation](docs/installation.md) and [deployment](docs/deployment.md). Linux deployment and
Chromium isolation checks are release prerequisites, alongside the native archive installer tests.

Rust source files are limited to 400 lines by a repository test. Generated projects also pin
their Rust and pnpm dependency graphs so repeated generation and production builds remain
reviewable and reproducible.

Signed remote modules support `install`, `update`, `verify`, `uninstall`, and `list`; see
`docs/module-registry.md` for the lockfile, trust pinning, and offline verification contract.

Before committing or pushing a change, inspect the worktree and run the checks that match the
files you changed. The advisory check requires `cargo-deny` 0.20.2 or newer:

```bash
git status --short --branch
git status --short --ignored
git diff --check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check advisories
scripts/run-template-build.sh
```

Specialized generated-backend, coverage, and package targets can accumulate several gigabytes. Run
`scripts/clean-test-artifacts.sh` to remove only those disposable artifacts, or pass `--all` to
delegate a complete workspace cleanup to `cargo clean`.

Do not commit local secrets or generated machine state. In particular, keep `.env` files (except
intentional `.env.example` templates), private keys and certificates, `node_modules/`, `target/`,
`references/`, Playwright reports, and `test-results/` out of Git. The repository `.gitignore`
already excludes these paths; verify tracked, untracked, and ignored state before the first push.

## License

Workspace crates are licensed under MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
