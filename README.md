# AppStruct

[English](README.md) | [简体中文](README.zh-CN.md)

AppStruct is a configuration-driven Rust full-stack application generator. It compiles a
multi-file YAML App Spec into a typed IR, PostgreSQL migrations, an Axum/SeaORM backend,
OpenAPI, a TypeScript client, and a React/Vite application.

The repository is currently a technical preview. It is distributed from a source checkout for now;
there is no crates.io package or binary installer yet. M0-M6 are complete, including production
builds, the coordinated development server, Tenant/Audit/Mail/Jobs/File modules, the locked
`appstruct/saas@1` preset, transactional project updates, and runnable templates. The SaaS preset
includes an Admin operations overview and guarded Jobs retry/replay controls; Billing remains out
of scope for preset version 1.

## Requirements

| Dependency | Version | Used for |
| --- | --- | --- |
| Rust | 1.98.0 with rustfmt and Clippy | CLI and generated backend |
| PostgreSQL | 17 recommended | migrations and generated API |
| Node.js | 24 recommended | generated Web application |
| pnpm | 11.25.0 | locked Web dependency installation |
| Docker Compose | Current release, optional | `database.dev.mode: managed` only |

The workspace and every generated project pin the Rust toolchain. Generated Web apps also commit a
pnpm lockfile so installs and builds stay reproducible. Release installers are documented in
[Installation](docs/installation.md).

## Install the CLI

From the repository root:

```bash
rustup toolchain install 1.98.0 --component clippy,rustfmt
cargo build --release --locked -p appstruct-cli
./target/release/appstruct --version
```

Use `target/release/appstruct` directly, or copy it onto an existing `PATH` directory:

```bash
install -d "$HOME/.local/bin"
install -m 0755 target/release/appstruct "$HOME/.local/bin/appstruct"
appstruct --version
```

Keep `--locked` so Cargo uses the committed workspace `Cargo.lock`.
`cargo install --path crates/appstruct-cli` is not the reproducible install path in this layout.

## Quick start

Pick one template. `minimal` uses a database you already run; `dashboard` and `saas` can start
managed PostgreSQL through Docker Compose.

### Existing PostgreSQL

```bash
appstruct new notes --template minimal
cd notes
cp .env.example .env
```

Edit `DATABASE_URL` in `.env`, then:

```bash
export DATABASE_URL=postgresql://user:password@127.0.0.1:5432/notes
appstruct migrate dev --accept
appstruct doctor
appstruct dev
```

External databases default to `database.dev.migration: unmanaged`, so run migrations before the
first start. `appstruct dev` then generates and builds the backend, installs locked Web
dependencies, and starts the API and Vite. The default URLs are `http://127.0.0.1:3000` and
`http://127.0.0.1:5173`; override them with `--api-port` and `--web-port`.

### Managed PostgreSQL

The `dashboard` template includes `compose.yaml` and lets AppStruct manage PostgreSQL:

```bash
appstruct new project-hub --template dashboard
cd project-hub
appstruct doctor
appstruct dev
```

Managed mode starts only the Compose `postgres` service. Services started in this session stop on
Ctrl-C; named volumes are kept. Already-running services are left running. Managed mode defaults
to `database.dev.migration: prompt` and asks only when work is pending.

### SaaS preset

```bash
appstruct new saas-demo --template saas
cd saas-demo
appstruct preset show
appstruct doctor
appstruct dev
```

The template locks `appstruct/saas@1` with Auth, Tenant, Audit, Mail, Jobs, and File. After
registration, create an organization and use the tenant-isolated, audited Project and Task
resources. Development uses capture mail, local files under `.appstruct/files`, and PostgreSQL
for Jobs/Outbox.

The first registered user is a `member`. Provision an admin once from a trusted host:

```bash
appstruct auth bootstrap-admin --email admin@example.com
```

Commit `appstruct.lock`; it stores the preset digest and exact module versions. Inspect overrides
with `appstruct preset show --expanded`.

## Import an existing schema

Create a review-only App Spec draft from a live PostgreSQL schema:

```bash
appstruct db pull --schema public --output spec/imported.yaml
```

The command reads `DATABASE_URL`, never changes the database or root `includes`, and refuses to
overwrite the output. Review unsupported-shape warnings and add entity access rules before
including the draft in `appstruct.yaml`. Use `--check` in CI, or `--diff` to print live changes
without writing the file.

## Development migrations

Declare database ownership and migration behavior in `appstruct.yaml`:

```yaml
database:
  provider: postgres
  dev:
    mode: managed
    migration: prompt # auto | prompt | never | unmanaged
```

- `auto` creates and applies safe online migrations.
- `prompt` asks only when work is pending.
- `never` runs read-only compatibility checks and blocks stale schemas.
- `unmanaged` skips AppStruct migration checks; the operator owns the database.

`mode: managed` starts local Compose PostgreSQL. `mode: external` never starts or stops it.
Production backend startup never runs migrations; use `migrate status` and an explicit
`migrate apply` in the release.

## Generated capabilities

The Web runtime is pinned to React 19, TypeScript, and Vite, with TanStack Query, Router, Table,
and Form plus Zod. Resource APIs support offset and cursor pagination, filters, sorts, relation
filters, aggregates, and group-by queries, all under actor, resource, and tenant policy.

Other contracts live in dedicated guides:

- [Resource queries](docs/data-querying.md), [schema indexes](docs/schema-indexes.md), and
  [seed data](docs/seeding.md)
- Auth, Tenant, Audit, Mail, Jobs, and File modules
- Bulk operations, saved views, soft delete, invitations, email verification, OAuth/OIDC, and
  personal API tokens
- Generated Axum/SeaORM backend, OpenAPI, and TypeScript client

## Commands

```text
appstruct new <name> --template minimal|dashboard|saas
appstruct schema
appstruct check [--deny-warnings] [--format text|json]
appstruct generate [--check] [--timings]
appstruct migrate plan|dev|lint|apply|status
appstruct dev [--api-port <port>] [--web-port <port>]
appstruct build
appstruct doctor [--format text|json]
appstruct db pull [--schema <name>] [--output <project-relative-path>] [--check | --diff]
appstruct auth bootstrap-admin --email <address>
appstruct preset show [--expanded]
appstruct update
```

`migrate plan` is read-only. `migrate dev --accept` creates and may apply only non-destructive
online migrations. Production uses `migrate status` then explicit `migrate apply`. `migrate lint`
reports destructive, locking, and unsafe non-null changes; use `--deny-warnings` in CI.
`appstruct generate --timings` prints compiler, planner, formatter, cache-hit, and output costs.

## Documentation

Each guide has an English source and a Simplified Chinese translation (`*.zh-CN.md`). The static
site in `site/` renders both languages; from the workspace root run `npm run site:dev`.

### Start here

- [Installation](docs/installation.md)
- [Upgrading](docs/upgrading.md)

### Build and delivery

- [Deployment](docs/deployment.md)
- [Releasing](docs/releasing.md)
- [Migration lint](docs/migration-lint.md)
- [Module registry](docs/module-registry.md)
- [Delivery optimization](docs/optimization-progress.md)

### Data modeling

- [Generated resource queries](docs/data-querying.md)
- [Lossless scalar values and datetime controls](docs/scalar-values.md)
- [Schema indexes](docs/schema-indexes.md)
- [Seed data](docs/seeding.md)
- [Aggregate line items](docs/aggregate-line-items-rfc.md)
- [Relation display](docs/relation-display.md)
- [Soft delete and history](docs/soft-delete.md)
- [Saved views](docs/saved-views.md)

### Application design

- [Headless Web controllers](docs/headless-controller.md)
- [Business UI semantics](docs/business-ui-semantics.md)
- [Bulk operations](docs/bulk-operations.md)
- [Entity workflows](docs/workflows.md)
- [Reports](docs/reports.md)
- [Record activity](docs/activity.md)

### Platform modules

- [Email verification](docs/email-verification.md)
- [Organization invitations](docs/organization-invitations.md)
- [OAuth and OIDC](docs/oauth-oidc.md)
- [Personal API tokens](docs/personal-api-tokens.md)
- [Realtime events, presence, and edit leases](docs/realtime.md)
- [Interval schedules](docs/schedules.md)
- [Signed webhooks](docs/webhooks.md)

### Operations

- [Operations Admin console](docs/admin-console.md)
- [Metrics and database workloads](docs/observability.md)
- [Chromium report renderer](docs/report-renderer.md)
- [Production report renderer adapter RFC](docs/report-renderer-adapter-rfc.md)
- [Operations Demo findings](docs/operations-demo-findings.md)

### Project records

- [Next product roadmap](docs/next-product-roadmap.md)
- [Product requirements](PRODUCT.en.md)
- [Technical design](TECHNICAL_DESIGN.en.md)

The `references/` directory contains local research material and is excluded from version control.

## Quality checks

Run repository gates with the pinned toolchain. Advisory checks need `cargo-deny` 0.20.2 or newer:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check advisories
scripts/run-template-build.sh
```

PostgreSQL browser tests use dedicated databases and the matching `scripts/run-*-e2e.sh` scripts.
GitHub Actions only builds and deploys the documentation site; run template, quality, and
PostgreSQL gates locally. `scripts/run-template-build.sh` checks generated Web production
dependencies, formatting, tests, types, and the Vite bundle.

`scripts/clean-test-artifacts.sh` removes disposable generated-backend, coverage, and package
targets. Pass `--all` to run `cargo clean` on the whole workspace.

Before committing:

```bash
git status --short --branch
git diff --check
```

Do not commit real `.env` files, passwords, API keys, private keys or certificates,
`node_modules/`, `target/`, `references/`, Playwright reports, or `test-results/`. Placeholder
values in `.env.example` may be committed.

## Current boundaries

AppStruct is a technical preview. Cutting a GitHub release still requires maintainer remote setup,
and production migrations are reviewed and applied as a separate release step. Billing, hosted
deployment adapters, and a visual editor are roadmap items, not stable preset v1 promises.

## License

Workspace crates are licensed under MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
