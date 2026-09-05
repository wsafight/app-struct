# AppStruct Next Product Roadmap

> Status: active implementation (Phase A is complete; Phase B correctness and operations are mostly complete)
> Date: 2026-08-30
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

The read-only `appstruct db pull` workflow now derives a reviewable App Spec draft from PostgreSQL.
The remaining gap is migration from real-world schemas: composite keys and cross-schema relations
still require manual modeling. Domains are imported as their base scalar with a constraint warning;
arrays, generated columns, and unsupported types are omitted with warnings rather than emitted as
runtime-incompatible fields. Access rules cannot be inferred from database metadata.

### 2.2 Data access and reporting

Generated resources now provide offset/cursor pagination, one-hop relation filters, bounded
aggregates/grouping, and field-level access. Remaining gaps are read-only computed fields, deeper
relation traversal, richer report/dashboard queries, and cursor traversal with user-selected sort
keys.

### 2.3 Admin productivity

The Web runtime now has explicit bulk operations, saved views, import/export, soft delete, restore,
revision-safe inline scalar editing, and audit-backed history with field-level diffs. Remaining
productivity gaps are record-scoped history navigation and a more complete reusable headless
form/URL controller for custom screens. Generated list/detail pages and custom page props now share
the first headless controller slice for query keys, permission gates, request state, refetching, and
mutation invalidation.

### 2.4 Operational administration

The implemented modules expose infrastructure capabilities but only limited operational UI. An
Admin module should complete organization/session operations, mail capture, file usage, and
record-scoped audit navigation before Billing is added to the SaaS preset.

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

- [x] Bulk update/delete and CSV import/export.
- [x] Browser-private saved list views and shareable URL query snapshots.
- [x] Server-backed private and team-shared saved views.
- [x] Soft delete, trash, restore, and audit-backed history display.
- [x] Revision-safe inline scalar editing and field-level audit snapshot diffs.
- [x] Organization invitations.
- [x] Email verification.
- [x] OAuth/OIDC.
- [x] Personal API tokens.
- [x] Jobs, mail, file, user, tenant, and audit administration pages (overview and module links).
- [x] Admin job inspection, dead-letter retry, and terminal-job replay.
- [x] Admin webhook delivery inspection, dead-letter retry, and terminal-delivery replay.
- [x] Interval schedules with skip-missed execution and stale-definition disable.
- [x] Signed webhook outbox with bounded HTTP timeouts.
- [x] Resource-authorized SSE, PostgreSQL multi-replica fan-out, presence, and optional TTL edit leases.

### Phase C: Distribution and delivery

- [x] Remote module registry lifecycle (`install`, `update`, `verify`, `uninstall`, and `list`) with
  `appstruct.modules.lock`, signature verification, offline cache validation, and compatibility checks.
- Deployment adapters and environment promotion without a mandatory hosted control plane.
- Billing and subscription operations.
- Visual schema, permission, page, and migration editor that produces reviewable App Spec diffs.
- Project-local agent instructions and a policy-governed MCP adapter.

### Next contract freeze

The next data and Web runtime slice must settle its public contracts before implementation:

1. Computed fields use typed, portable IR expressions rather than embedding unchecked SQL in App
   Spec. The contract must define supported operators, null propagation, field dependencies, and
   read-access behavior before adding syntax to the schema.
2. Sorted cursor tokens bind the ordered sort specification and typed key values, define null
   ordering, and always include the primary key as the final tie-breaker. A cursor from a different
   filter or sort contract must be rejected.
3. Server-backed saved views record an owner, resource, revision-guarded query state, and `private`
   or tenant-scoped `team` visibility. Team views are creator-writable and organization-readable;
   outside tenant mode only private and browser-local views are available.
4. The headless controller becomes complete only when it owns URL query parsing plus form
   validation, field errors, revision conflicts, and unsaved-change state. Generated pages and
   custom pages must consume the same controller contract.

Cursor mode remains primary-key ordered, and custom forms continue to own their form and URL state.

## 4. First Slice: `appstruct db pull`

### 4.1 Command contract

```text
appstruct db pull [--schema public] [--output spec/imported.yaml] [--check | --diff]
```

The command:

- discovers an existing AppStruct project;
- reads `DATABASE_URL` from the process environment or project `.env`, with the process value
  taking precedence;
- connects with the same TLS policy as migration commands;
- performs only PostgreSQL catalog reads;
- writes a deterministic domain Spec draft;
- compares an existing draft without writing when `--check` or `--diff` is selected;
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

Composite primary keys, composite foreign keys, and cross-schema relations are reported as
warnings. Domains are lowered to supported base scalars with a constraint warning. Arrays,
generated columns, expressions, and unsupported PostgreSQL types are omitted with warnings. Tables
without exactly one primary-key column are omitted because AppStruct entities require a single key.
The draft is never added to `includes` automatically.

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
