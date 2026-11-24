# AppStruct

[English](README.md) | [简体中文](README.zh-CN.md)

**The Deterministic Full-Stack Compiler for AI Coding Agents — and Instant Secure Admin for PostgreSQL.**

> **You write YAML (or let your AI agent write it). You don't need to know Rust to ship.**

AppStruct compiles a declarative, multi-file YAML App Spec into a typed Intermediate Representation (IR), forward-only PostgreSQL migrations, a fail-closed Rust (Axum/SQLx) backend, OpenAPI, a TypeScript client, and a React 19 / Rsbuild web application.

---

## Why AppStruct?

### 1. Stop AI Coding Agents from Shipping Broken Auth & Migrations
When AI agents hand-write full-stack CRUD, migrations, and access checks, they routinely miss tenant filters, leak restricted fields, or generate destructive DDL.

AppStruct turns AI full-stack development into a **closed, verifiable compiler loop**:

```text
AI writes declarative YAML Spec
  → Built-in MCP Server (`appstruct mcp`) + Draft 2020-12 JSON Schema
  → `appstruct check --deny-warnings` + `appstruct access preview`
  → Deterministic Rust + PostgreSQL + React/TS compilation
```

### 2. Turn Any Existing PostgreSQL Database into a Secure Admin in 3 Minutes
Already have a PostgreSQL database? Point `appstruct db pull` at it to generate a fail-closed App Spec draft, inspect access policies, and ship a single-binary Rust + React admin console — **no per-seat SaaS tax, no vendor lock-in, full source ownership**.

```bash
appstruct db pull --schema public --output spec/imported.yaml
appstruct access preview --role admin
appstruct migrate baseline --accept
appstruct dev
```

---

## Try the Live Demo in One Command (Zero Host Prerequisites)

No local Rust, Node.js, pnpm, or PostgreSQL required — just Docker:

```bash
docker compose -f docker-compose.demo.yaml up --build
```

Open **http://localhost:8080** to explore the full multi-tenant SaaS demo (Auth, Organizations, RBAC, Audit log, Projects/Tasks, Jobs/Outbox, and Admin operations console).

---

## Use with AI Coding Agents (MCP + Skills + Rules)

AppStruct ships a **built-in zero-dependency MCP stdio server** (`appstruct mcp`) plus ready-to-use agent rules:

- **Claude Code**:
  ```bash
  claude mcp add appstruct -- appstruct mcp
  ```
- **Cursor / Windsurf / Antigravity / Codex**:
  - MCP server command: `appstruct mcp` (optional `--project /path/to/app`)
  - Included rules & skills: [`.cursor/rules/appstruct.mdc`](.cursor/rules/appstruct.mdc), [`.claude/skills/appstruct/SKILL.md`](.claude/skills/appstruct/SKILL.md), and generated `AGENTS.md` in every project.

**Exposed MCP tools:**
- `appstruct_schema` — Draft 2020-12 App Spec JSON Schema (filterable by section)
- `appstruct_check` — Spec + IR + user symbol validation with structured diagnostics
- `appstruct_access_preview` — Declarative CRUD, field, owner, and tenant access matrix
- `appstruct_generate` — Atomic regeneration (or `--check` drift verification)
- `appstruct_migrate_plan` — Schema diff, SQL preview, and migration lint
- `appstruct_db_pull_preview` — Read-only PostgreSQL schema introspection preview

---

## Local Requirements & Installation

| Dependency | Version | Used for |
| --- | --- | --- |
| Rust | `>= 1.89` (Edition 2024, with `rustfmt` & `clippy`) | CLI and generated backend |
| PostgreSQL | `16` / `17` recommended | Migrations and generated API |
| Node.js | `>= 20.0.0` | Generated Web application |
| pnpm | `>= 9.0.0` | Web dependency installation |
| Docker Compose | Optional | `database.dev.mode: managed` or one-command demo |

Build the CLI from source:

```bash
cargo build --release --locked -p appstruct-cli
install -d "$HOME/.local/bin"
install -m 0755 target/release/appstruct "$HOME/.local/bin/appstruct"
appstruct --version
```

> Prefer a zero-install dev environment? Open this repository in VS Code with the **Dev Containers** extension and select **Reopen in Container**. See [Installation](docs/installation.md).

---

## Quick Start

### Create a New App (`minimal`, `dashboard`, or `saas`)

```bash
# Multi-tenant SaaS preset (Auth, Tenant, Audit, Mail, Jobs, File) with managed Postgres
appstruct init my-saas --template saas
cd my-saas
appstruct doctor
appstruct dev
```

Or connect to an **existing PostgreSQL database**:

```bash
appstruct init my-admin --template minimal --database-mode external
cd my-admin
cp .env.example .env # set DATABASE_URL
appstruct db pull --schema public --output spec/imported.yaml --access role --role admin
# Add spec/imported.yaml to appstruct.yaml `includes`, then:
appstruct check --deny-warnings
appstruct migrate baseline --accept
appstruct dev
```

Default local URLs:
- Web UI: `http://127.0.0.1:5173`
- API & OpenAPI: `http://127.0.0.1:3000` (`/openapi.json`)

Bootstrap the first admin user from a trusted host:

```bash
appstruct auth bootstrap-admin --email admin@example.com
```

---

## Core Architecture & Guarantees

- **Fail-Closed Authorization**: Every entity explicitly declares `list`, `read`, `create`, `update`, and `delete` rules (`public`, `authenticated`, `roles`, `owner`), plus field-level read/write rules and tenant isolation. Verified against a 2,112-case PostgreSQL permission truth table.
- **Clear Ownership Boundaries**:
  - **You own**: `appstruct.yaml`, `spec/*.yaml`, `app/backend/` (custom Rust policies, hooks, `AfterCommitHook`, routes), `app/web/` (custom React components & pages), `migrations/*.sql`.
  - **Compiler owns**: `generated/` (atomic, SHA-256 verified, never hand-edited).
- **Safe Online Migrations**: `migrate plan`, `migrate lint --deny-warnings`, `migrate dev --accept`, `migrate baseline --accept`, and explicit `migrate apply` in production releases.
- **Modern Generated Stack**:
  - **Backend**: Rust, Axum, SQLx, `sift-query`, optimistic concurrency (`If-Match: "rev-<n>"`), offset & cursor pagination, relation filters, aggregates, group-by, workflows, signed webhooks, jobs/outbox, schedules.
  - **Frontend**: React 19, TypeScript, Rsbuild/Rspack, Rstest, TanStack Query / Router / Table / Form.

---

## CLI Command Reference

```text
appstruct init [<name>] [--template minimal|dashboard|saas] [--database-mode external|managed]
appstruct mcp                                         # Built-in stdio MCP server for AI agents
appstruct schema                                      # Emit Draft 2020-12 App Spec JSON Schema
appstruct check [--deny-warnings] [--format json]     # Validate Spec, IR, and user symbols
appstruct access preview [--entity <id>] [--role <r>] # Preview effective CRUD/field/tenant rules
appstruct generate [--check] [--timings]              # Regenerate backend, web, OpenAPI, IR
appstruct migrate plan|lint|dev|baseline|apply|status # Forward-only PostgreSQL migrations
appstruct db pull [--schema public] [--review|--diff] # Introspect live PostgreSQL into Spec
appstruct dev [--api-port 3000] [--web-port 5173]     # Coordinated local dev server
appstruct build                                       # Production build + release manifest
appstruct deploy verify                               # Verify immutable release manifest
appstruct doctor [--format json]                      # Environment & database diagnostics
appstruct auth bootstrap-admin --email <address>      # Provision initial admin
appstruct preset show [--expanded]                    # Inspect locked preset configuration
```

---

## Documentation

Every guide is maintained in English and Simplified Chinese (`*.zh-CN.md`). Preview the docs site locally with `pnpm run site:dev`.

- **Getting Started**: [Installation](docs/installation.md) · [Existing PostgreSQL Guide](docs/existing-database.md) · [Upgrading](docs/upgrading.md)
- **Data & Queries**: [Resource Queries](docs/data-querying.md) · [Schema Indexes](docs/schema-indexes.md) · [Seeding](docs/seeding.md) · [Soft Delete](docs/soft-delete.md) · [Saved Views](docs/saved-views.md)
- **Business Logic & UI**: [Workflows](docs/workflows.md) · [Bulk Operations](docs/bulk-operations.md) · [Headless Controllers](docs/headless-controller.md) · [Reports](docs/reports.md) · [Activity](docs/activity.md)
- **Platform Modules**: [Auth & OAuth/OIDC](docs/oauth-oidc.md) · [Invitations](docs/organization-invitations.md) · [Personal API Tokens](docs/personal-api-tokens.md) · [Webhooks](docs/webhooks.md) · [Schedules](docs/schedules.md) · [Realtime](docs/realtime.md) · [Billing](docs/billing.md)
- **Deployment & Ops**: [Deployment](docs/deployment.md) · [Migration Lint](docs/migration-lint.md) · [Admin Console](docs/admin-console.md) · [Observability](docs/observability.md) · [CI Quality Gates](docs/continuous-integration.md)
- **Architecture & Roadmap**: [Next Product Roadmap](docs/next-product-roadmap.md) · [Product Requirements](PRODUCT.en.md) · [Technical Design](TECHNICAL_DESIGN.en.md)

---

## Quality Gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/run-template-build.sh
```

## License

Dual-licensed under **MIT OR Apache-2.0**. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
