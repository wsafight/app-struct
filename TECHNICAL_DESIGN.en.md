# AppStruct Technical Architecture

> **Single source of truth**: Operational and reference guides live under `docs/`. Current format versions are tracked in [docs/compatibility.md](docs/compatibility.md).

## Pipeline
1. **App Spec (YAML)** validated against Draft 2020-12 JSON Schema (`crates/appstruct-compiler/schema/appstruct.schema.json`).
2. **Compiler (`appstruct-compiler`)** lowers root + domain specs into canonical Typed IR (`appstruct-ir`) with deterministic diagnostics.
3. **Schema & Migrations (`appstruct-schema`, `appstruct-migrate`)** extract PostgreSQL DDL (including optional `ENABLE/FORCE ROW LEVEL SECURITY` tenant policies), lint destructive changes, and introspect existing PostgreSQL tables, views, and materialized views.
4. **Codegen (`appstruct-codegen`)** deterministically emits Axum + SQLx Rust backend, typed TypeScript client, React UI, and OpenAPI spec with SHA-256 ownership isolation (`appstruct.ownership.json`).
5. **CLI & MCP (`appstruct-cli`)** provides `init`/`new`, `check`, `build`, `dev`, `db pull`, `migrate`, `deploy` (Fly.io, Railway, Coolify, Caddy), `access preview`/`access matrix`, `update` (Git-clean check + atomic directory replace), and `mcp` (JSON-RPC 2.0 stdio server).

## Extensibility (Dual-Track)
- **Out-of-process HTTP Plugins**: Declarative HTTP Commands/Queries (HMAC-SHA256 signed, timeouts, retries) and external HTTP report renderers (`renderer: http`) allow Python/Node/Go/Rust services without bloating the core runtime.
- **Compile-time Composed Modules**: Full-stack modules (`auth`, `tenant`, `billing`, `audit`, `file`, `jobs`, `mail`, `webhooks`) compose natively via Cargo and pnpm lockfiles.
