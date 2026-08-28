# AppStruct Next Product Roadmap

> Status: active implementation (`db pull`, cursor/relation queries, aggregates, and field access complete)
> Date: 2026-08-28
> Scope: product work after the M0-M6 technical preview baseline

## 1. Outcome

AppStruct already has a reliable application compiler, migration workflow, generated REST API,
React CRUD runtime, authentication, tenant isolation, audit, mail, jobs, and file modules. The next
product phase should make those foundations useful for existing databases and repeated operational
work rather than expanding the supported technology matrix.

The recommended order is:

1. Existing database onboarding and richer data access.
2. High-frequency admin workflows and operational consoles.
3. Automation, distribution, deployment, and visual tooling.

GraphQL, additional databases, runtime-loaded Rust plugins, and CMS-specific page builders are not
near-term priorities. Query capabilities should first be represented in the shared IR so REST,
OpenAPI, generated clients, and any future protocol adapter retain identical authorization rules.

## 2. Product Gaps

### 2.1 Existing database onboarding

The product describes existing-database admin generation as a target scenario, but it cannot yet
derive an App Spec from PostgreSQL. The first addition is a read-only `appstruct db pull` workflow.

### 2.2 Data access and reporting

Generated resources currently provide offset pagination, basic filters, search, sorting, and
bounded relation expansion. Business applications also need cursor pagination, relation filters,
aggregates, grouping, read-only computed values, and dashboard queries. Every query operation must
apply the same actor, tenant, and resource scopes as ordinary list reads, including aggregate
counts.

### 2.3 Admin productivity

The Web runtime needs explicit bulk operations, saved views, import/export, inline editing, soft
delete, restore, and record history. Bulk operations require per-record authorization, explicit
partial-failure semantics, and optimistic concurrency contracts; they must not be implemented as a
client loop over unrestricted endpoints.

### 2.4 Operational administration

The implemented modules expose infrastructure capabilities but only limited operational UI. An
Admin module should cover users, organizations, invitations, sessions, job retry/dead-letter state,
mail capture, file usage, and audit diffs before Billing is added to the SaaS preset.

### 2.5 Automation and ecosystem

The Jobs outbox is the natural base for schedules, database-change events, signed webhooks, replay,
and server-sent updates. Third-party modules should be downloaded and verified at build time with a
locked checksum and compatibility contract. Production runtime loading of dynamic Rust libraries
remains out of scope.

## 3. Delivery Plan

### Phase A: Data onboarding

- [x] `appstruct db pull` for PostgreSQL tables, columns, keys, supported defaults, enums, and relations.
- [x] Cursor pagination and filters across relations.
- [x] Count/sum/average/min/max aggregates and group-by queries.
- [x] Composite and partial indexes.
- [x] Seed data.
- [x] Stronger migration linting.
- [x] Field-level read/write access rules.

### Phase B: Usable operations

- Bulk update/delete and CSV import/export.
- Private and shared saved list views.
- Soft delete, trash, restore, and audit-backed history display.
- Organization invitations, email verification, OAuth/OIDC, and personal API tokens.
- Jobs, mail, file, user, tenant, and audit administration pages.
- Recurring schedules, signed webhooks, retry, and replay.
- Optional SSE updates and record presence/locks.

### Phase C: Distribution and delivery

- Remote module registry with lockfile, checksum, signature, and compatibility validation.
- Deployment adapters and environment promotion without a mandatory hosted control plane.
- Billing and subscription operations.
- Visual schema, permission, page, and migration editor that produces reviewable App Spec diffs.
- Project-local agent instructions and a policy-governed MCP adapter.

## 4. First Slice: `appstruct db pull`

### 4.1 Command contract

```text
appstruct db pull [--schema public] [--output spec/imported.yaml]
```

The command:

- discovers an existing AppStruct project;
- reads `DATABASE_URL` from the process environment or project `.env`, with the process value
  taking precedence;
- connects with the same TLS policy as migration commands;
- performs only PostgreSQL catalog reads;
- writes a deterministic domain Spec draft;
- refuses absolute output paths, parent traversal, symlinks, and existing destinations;
- does not change `appstruct.yaml`, `includes`, migrations, snapshots, locks, or generated files.

Machine-readable output uses the standard success/error envelope and reports the schema, output
path, table/entity count, and warnings.

### 4.2 Supported first-version shapes

- Base tables in one explicitly selected PostgreSQL schema.
- Exactly one primary-key column per generated entity.
- UUID, character, text, integer, bigint, decimal, boolean, date, timestamp, JSON/JSONB, and native
  PostgreSQL enum columns.
- Identity/serial integer generation and current-time timestamp defaults.
- Single-column unique constraints.
- Single-column foreign keys that target the selected schema and a generated entity.
- `RESTRICT`/`NO ACTION`, `CASCADE`, and `SET NULL` delete behavior.

Quoted database identifiers are normalized into portable AppStruct names while preserving the
original table or column through explicit `table` and `column` declarations.

### 4.3 Review-required shapes

Composite primary keys, composite foreign keys, cross-schema relations, arrays, domains, generated
columns, expressions, and unsupported PostgreSQL types are reported as warnings. Tables without
exactly one primary-key column are omitted because AppStruct entities require a single key.
Unsupported scalar columns are represented as JSON placeholders with a nearby review comment; the
draft is never added to `includes` automatically.

Access rules cannot be inferred from a database schema. Generated entities intentionally omit
`access`, so adding the draft to an application fails closed until the developer declares public,
authenticated, role, or owner policies.

### 4.4 Acceptance criteria

1. Pulling the same unchanged database twice into different paths produces identical bytes.
2. A typical UUID parent/child schema renders valid entity, field, enum, unique, generated, and
   relation declarations.
3. Missing `DATABASE_URL`, connection failures, invalid schema names, unsafe output paths, and
   existing files fail without filesystem mutation.
4. Unsupported database shapes produce deterministic warnings without exposing the connection URL.
5. Text and JSON command modes follow the existing CLI contracts.
6. Unit tests cover catalog-to-model mapping and rendering; a PostgreSQL integration test covers the
   command when the repository database test environment is available.
