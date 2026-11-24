---
name: appstruct
description: Build, modify, validate, and migrate AppStruct full-stack applications from declarative YAML App Specs using deterministic compilation, fail-closed RBAC/tenant policies, and the AppStruct MCP server.
---

# AppStruct Skill for AI Coding Agents

Use this skill whenever the user asks to create, extend, debug, or migrate an **AppStruct** application (`appstruct.yaml`, `spec/*.yaml`, `app/backend/`, `app/web/`, or existing PostgreSQL import via `db pull`).

**Core principle**: Do not hand-write boilerplate CRUD, SQL migrations, or ad-hoc auth checks. Express domain models, relations, workflows, indexes, and access rules in declarative YAML, validate them deterministically with the AppStruct compiler, and let `appstruct generate` emit the Rust backend, PostgreSQL migrations, OpenAPI schema, and React/TypeScript frontend.

## 1. Connect the Built-in MCP Server (Recommended)

AppStruct ships a zero-dependency stdio MCP server directly inside the CLI:

```bash
# Claude Code
claude mcp add appstruct -- appstruct mcp

# Or pass an explicit project path
claude mcp add appstruct -- appstruct --project /path/to/app mcp
```

Available MCP tools:
- `appstruct_schema` — fetch the Draft 2020-12 App Spec JSON Schema (filterable by `section`: `app`, `domain`, `entity`, `field`, `access`, `modules`, `workflow`).
- `appstruct_check` — validate `appstruct.yaml` + `spec/*.yaml` + user symbol bindings; returns machine-readable diagnostics (`code`, `message`, `path`, `help`).
- `appstruct_access_preview` — evaluate declarative CRUD, field, owner, and tenant access decisions for sample roles/actors.
- `appstruct_generate` — run `appstruct generate` (or `check: true` to verify no drift).
- `appstruct_migrate_plan` — diff target schema against `.appstruct/schema.snapshot.json`, emit SQL preview, and run migration lint.
- `appstruct_db_pull_preview` — inspect a live PostgreSQL schema (`DATABASE_URL`) and return a read-only App Spec draft preview.

Available MCP resources:
- `appstruct://schema/app-spec` — complete App Spec JSON Schema.
- `appstruct://guide/agent` — project ownership and security contract (`AGENTS.md`).

## 2. Deterministic Development Workflow

Follow this exact loop for every feature or schema change:

1. **Inspect exact Spec syntax** (never guess YAML keys):
   - Call `appstruct_schema` or run `appstruct schema`.
2. **Edit only user-owned files**:
   - `appstruct.yaml` / `spec/*.yaml` — entities, fields, relations, workflows, indexes, access policies, modules.
   - `app/backend/` — custom Rust `EntityPolicy`, lifecycle hooks, `AfterCommitHook`, job handlers, custom Axum routes (`AppExtensions::builder()`).
   - `app/web/` — custom UI components (`registerFieldComponent`), custom pages (`registerPage`), locale/theme.
   - **Never edit** `generated/`, `.appstruct/`, `appstruct.lock`, or `appstruct.modules.lock`.
3. **Validate with zero warnings**:
   - Call `appstruct_check` (`deny_warnings: true`) or run `appstruct check --deny-warnings`.
   - Fix every diagnostic before proceeding.
4. **Audit access rules before generating**:
   - Call `appstruct_access_preview` or run `appstruct access preview --entity <Domain::Entity> --role member`.
   - Confirm unauthenticated/unauthorized roles are denied (fail-closed) and tenant boundaries hold.
5. **Regenerate deterministically**:
   - Call `appstruct_generate` or run `appstruct generate && appstruct generate --check`.
6. **Plan and lint migrations**:
   - Call `appstruct_migrate_plan` or run `appstruct migrate plan && appstruct migrate lint --deny-warnings`.
   - Accept safe development migrations with `appstruct migrate dev --accept`.

## 3. Existing PostgreSQL Onboarding (`db pull`)

When connecting AppStruct to an existing PostgreSQL database:
1. Run `appstruct db pull --schema public --output spec/imported.yaml` (or preview via `appstruct_db_pull_preview`).
2. Review unsupported-shape warnings and add explicit fail-closed access rules (or pass `--access role --role admin` / `--access authenticated`).
3. Include `spec/imported.yaml` in `appstruct.yaml`.
4. Run `appstruct migrate baseline --accept` — **never** apply an initial `CREATE TABLE` migration to an already populated database.

## 4. Non-Negotiable Security & Concurrency Invariants

- **Fail-closed authorization**: Every entity must explicitly declare `list`, `read`, `create`, `update`, and `delete`. Custom `EntityPolicy` may only narrow access and must never bypass tenant scope.
- **Optimistic concurrency**: Record mutations (`PATCH`, `DELETE`, workflow transitions) require `If-Match: "rev-<n>"`.
- **Transaction safety**: Run DB queries inside `before_*` / `after_*` hooks through `RequestContext`'s transaction; move irreversible side effects (HTTP webhooks, emails, file deletes) to `AfterCommitHook` or the Jobs/Outbox table.
- **Secrets stay out of Spec**: Never put credentials, API keys, or `DATABASE_URL` in `appstruct.yaml`, `spec/*.yaml`, or committed `.env` files.
