# AppStruct Project Guide for AI Coding Agents

This repository is an **AppStruct** application. Follow these ownership boundaries, validation commands, and security contracts whenever modifying specifications, migrations, or custom extensions.

## 1. File Ownership Boundaries

- **User-owned (safe to edit)**:
  - `appstruct.yaml` and `spec/*.yaml` — root configuration, enabled modules, domain entities, fields, relations, workflows, indexes, and access policies.
  - `app/backend/` — custom Rust policies, lifecycle hooks, after-commit hooks, job handlers, and custom Axum routes wired via `AppExtensions::builder()`.
  - `app/web/setup.ts` (and files under `app/web/`) — UI locale, theme, field display labels, custom field components (`registerFieldComponent`), and custom routes (`registerPage`).
  - `migrations/*.sql` — reviewed forward-only PostgreSQL migrations bound to `.appstruct/schema.snapshot.json`.
  - `Dockerfile`, `compose.production.yaml`, `deploy/*` — deployment configuration (compare against CLI defaults with `appstruct template diff`).
- **Generated or CLI-managed (NEVER edit by hand)**:
  - `generated/` — regenerated atomically by `appstruct generate` (`backend/`, `web/`, `openapi/openapi.json`, `ir/app.ir.json`).
  - `.appstruct/` — snapshots, build manifests, and caches.
  - `appstruct.lock` — managed by AppStruct CLI commands.

## 2. Standard Development & Verification Loop

If your coding agent supports MCP, connect the built-in stdio server (`appstruct mcp`) to call `appstruct_schema`, `appstruct_check`, `appstruct_access_preview`, `appstruct_access_matrix`, `appstruct_impact`, `appstruct_generate`, `appstruct_migrate_plan`, `appstruct_migrate_lint`, and `appstruct_db_pull` directly.

Always inspect the machine-readable schema and validate before running or committing:

```bash
# 1. Inspect the exact App Spec JSON Schema when adding Spec syntax
appstruct schema

# 2. Validate Spec + IR invariants (zero warnings)
appstruct check --deny-warnings

# 3. Preview effective CRUD, field, owner, and tenant access rules for roles
appstruct access preview --entity <Domain::Entity> --role member

# 4. Regenerate backend, web, and OpenAPI artifacts
appstruct generate
appstruct generate --check

# 5. Plan, lint, and accept forward-only PostgreSQL migrations
appstruct migrate plan
appstruct migrate lint --deny-warnings
appstruct migrate dev --accept
appstruct migrate status
```

For existing PostgreSQL databases, use `appstruct db pull --schema public --output spec/imported.yaml` and `appstruct migrate baseline` (never apply an initial `CREATE TABLE` migration directly to a populated database).

## 3. Security & Architecture Invariants

1. **Fail-closed authorization**: Every entity must declare explicit `list`, `read`, `create`, `update`, and `delete` rules (`public`, `authenticated`, `roles`, or `owner`). Custom `EntityPolicy` narrows access and must never bypass tenant boundaries.
2. **Optimistic concurrency**: Record mutations (`PATCH`, `DELETE`, workflow transitions, aggregate edits) require `If-Match: "rev-<n>"`. Preserve `revision` checks in custom UI flows.
3. **Transaction safety**: Inside `before_*` and `after_*` hooks, execute database queries through the provided `RequestContext` transaction. Never perform irreversible external side effects (HTTP calls, object deletion) inside a rolled-back transaction; use `AfterCommitHook` or the Jobs/Webhooks outbox.
4. **Secrets stay in environment variables**: Never place credentials, API keys, webhook secrets, or `DATABASE_URL` values in `appstruct.yaml`, `spec/*.yaml`, or committed `.env` files. Keep placeholder names in `.env.example`.
5. **Production verification**: Run `appstruct build` and `appstruct deploy verify` to produce and verify `.appstruct/release-manifest.json`, apply migrations as a separate release step (`appstruct migrate apply`), and run `node deploy/smoke.mjs <origin>`.
