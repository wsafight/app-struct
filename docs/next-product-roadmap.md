# AppStruct Next Product Roadmap

> Status: active implementation (Phases A and B are complete; first customer-facing delivery and remaining acceptance boundaries are in section 6)
> Date: 2026-11-05
> Scope: product work after the M0-M6 technical preview baseline

## 1. Outcome

AppStruct already has a reliable application compiler, migration workflow, generated REST API,
React CRUD runtime, authentication, tenant isolation, audit, mail, jobs, and file modules. The
primary product entry is now fixed as "existing PostgreSQL to a secure admin application with
authorization, audit, and operations UI." The next phase should improve completion of that journey
rather than expand the supported technology matrix.

The recommended order is:

1. Existing database onboarding and richer data access.
2. High-frequency admin workflows and operational consoles.
3. Automation, distribution, deployment, and visual tooling.

GraphQL, additional databases, runtime-loaded Rust plugins, and CMS-specific page builders are not
near-term priorities. Query capabilities should first be represented in the shared IR so REST,
OpenAPI, generated clients, and any future protocol adapter retain identical authorization rules.

The November 5, 2026 source review of Wasp and Open SaaS proposes an additional operable SaaS
journey: public marketing and pricing pages, personal accounts, paid feature enforcement, user
files, and business dashboards. Existing-database onboarding remains an entry point, sharing the
same core. See the [product capability review (Chinese)](wasp-opensaas-product-review.zh-CN.md)
for pinned sources and template limitations, and section 6 for the proposed delivery order.

## 2. Product Gaps

### 2.1 Existing database onboarding

The `appstruct db pull` workflow now derives a reviewable App Spec draft from PostgreSQL. `--review`
presents fields and relations table by table, confirms selected tables, and shows warnings before writing.
The remaining gap is migration from real-world schemas: composite keys and cross-schema relations
still require manual modeling. Domains are imported as their base scalar with a constraint warning;
arrays, generated columns, and unsupported types are omitted with warnings rather than emitted as
runtime-incompatible fields. Access rules cannot be inferred from database metadata.

Explicit `migrate baseline` now compares without writes and can record a matching canonical initial
migration without executing CREATE SQL. Includes, per-operation access and revision still require
review. It does not repair source schemas or accept seeds, custom SQL or migration chains. See the
[existing database guide](existing-database.md).

### 2.2 Data access and reporting

Generated resources now provide offset/cursor pagination, one-hop relation filters and aggregate
dimensions, deterministic Top-N grouping, declarative KPI/bar/donut charts, and field-level access.
Remaining gaps are read-only computed fields and deeper relation traversal, plus
richer cross-resource dashboards. Nested filters, projection, signed multi-column cursors and UTC day/week/month reports are implemented.

### 2.3 Admin productivity

The Web runtime now has explicit bulk operations, saved views, import/export, soft delete, restore,
revision-safe inline scalar editing, and paginated record history with field-level diffs inside
detail pages. Generated and custom pages share URL and form controllers, including validation,
conflicts and drafts. Import and bulk actions support review, per-item results and failed-item retries;
saved views also retain report settings.

### 2.4 Operational administration

Admin provides user session revocation, organization membership and invitations, mail delivery,
file metadata, Jobs, Schedules, Webhooks, Billing events, and global audit pages. Resource detail
pages provide record-scoped audit navigation. The next operations priority is deterministic release
manifests and `appstruct deploy verify` so Staging and Production promote the same immutable
artifacts, rather than another generic console.

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
- [x] One-hop relation dimensions, deterministic Top-N ordering, and declarative resource charts.
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
- [x] User session revocation, organization membership/invitation operations, and record history navigation.
- [x] Admin job inspection, dead-letter retry, and terminal-job replay.
- [x] Admin webhook delivery inspection, dead-letter retry, and terminal-delivery replay.
- [x] Interval schedules with skip-missed execution and stale-definition disable.
- [x] Signed webhook outbox with bounded HTTP timeouts.
- [x] Resource-authorized SSE, PostgreSQL multi-replica fan-out, presence, and optional TTL edit leases.

### Phase C: Distribution and delivery

- [x] Remote module registry lifecycle (`install`, `update`, `verify`, `uninstall`, and `list`) with
  `appstruct.modules.lock`, signature verification, offline cache validation, and compatibility checks.
- [x] Deterministic release manifests and pre-promotion artifact verification without a mandatory hosted control plane.
- Platform-specific deployment adapters, admitted only after three pilots repeat the same platform workflow.
- Billing and subscription operations (Stripe Billing v1 is supported; metered usage remains out of scope).
- Visual schema, permission, page, and migration editor that produces reviewable App Spec diffs
  (deferred until interviews identify text review as a top-three abandonment cause).
- Project-local agent instructions and a policy-governed MCP adapter (deferred until core CLI/API
  contracts are stable and three real projects request the same automation journey).

### Next contract freeze

The next data and Web runtime slice must settle its public contracts before implementation:

1. Computed fields use typed, portable IR expressions rather than embedding unchecked SQL in App
   Spec. The contract must define supported operators, null propagation, field dependencies, and
   read-access behavior before adding syntax to the schema.
2. Sorted cursor tokens bind the ordered sort specification and typed key values, define null
   ordering, and always include the primary key as the final tie-breaker. A cursor from a different
   filter or sort contract must be rejected.
3. The headless controller becomes complete only when it owns URL query parsing plus form
   validation, field errors, revision conflicts, and unsaved-change state. Generated pages and
   custom pages must consume the same controller contract.

Signed multi-column cursors and shared form/URL controllers are implemented. Management tables retain page pagination and exact totals.

Composite keys, cross-schema relations, computed fields, and deeper traversal do not enter the IR merely because real databases may need them. Each requires samples from
three real projects, a recorded failed journey, an explanation of why existing extension points are
insufficient, and a compatibility/authorization contract draft. Without that evidence, improve
warnings, review, and manual-modeling guidance first.

## 4. First Slice: `appstruct db pull`

### 4.1 Command contract

```text
appstruct db pull [--schema public] [--output spec/imported.yaml] [--review | --check | --diff]
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

## 5. Evidence for further investment

The 2026-10-27 baseline, authentication limits, theme/messages, data lifecycle, extension example,
template diff and frontend validation changes are implemented; see [delivery optimization](optimization-progress.md).
[Platform acceptance](continuous-integration.md) and [real-user observations](product-journeys.md)
track their pending executions separately. The following conditional ideas are not default delivery blockers.

| Direction | Current boundary and evidence needed |
| --- | --- |
| Computed fields, conditional validation and complex databases | Conditional requirements and sibling comparisons are available in [form validation](form-validation.md); cross-record rules and record/state-based field permissions use extension code. Composite keys, cross-schema and deeper relations follow the real-project sample gate above. |
| Boards, calendars, cross-resource dashboards and complete i18n | Startup English/Chinese standard copy, field display labels and shared formatting are available. Runtime language switching, multilingual Spec and new page types still require repeated project evidence. |
| User files, direct, multipart and scanned uploads | Generic user uploads and business record attachments enter the proposed section 6 product slice. Direct uploads need initiation/completion contracts; multipart and scanning remain separately scoped. Reference collection and storage reconciliation do not deliver the upload product. |
| Job priorities and general workflows | PostgreSQL Jobs already has named queues, leases, retries, dead letters and replay; Workflow is a single-entity state machine. Measure queueing and recovery before inferring capacity from storage choice. |
| Redis sessions and database RLS | Establish pool, authentication latency or isolation requirements. RLS must cover database roles, transactions, workers and administrator paths; tenant isolation currently runs in the application. |
| List totals and aggregate cost | Exact COUNT remains the default; group limits do not bound scans. Use real distributions, indexes and query plans before choosing estimates, timeouts or pagination changes. A million-row benchmark option is not a completed million-row acceptance test; the first single-host reference run and its query plans are in [observability](observability.md#million-row-reference-run). |
| Executable third-party modules | Remote packages distribute static Artifacts. Define compilation, registration, dependencies, migrations and uninstall before executable code; dynamic libraries or WASM are not default prerequisites. |
| Billing, platform adapters, Playground, editors, AI/MCP and hosting | Stripe Billing v1 already supports subscriptions. Business entitlement enforcement and a bounded quota enter the proposed section 6 slice. Rank further providers, complex billing and new delivery surfaces separately. |

Database recovery prioritizes verified backup restoration and forward repair without requiring automatic
down SQL. Optimize tests using duration, failure rate and duplicate coverage. Define support for Tenant,
Jobs, File, Mail and Billing from demand and maintenance cost rather than removing modules by count.

## 6. First Product Capability Delivery (2026-11-05)

The first implementation follows the source comparison order. The table records the first slice; the same-day P0–P2 follow-up supersedes its pending items in section 7. Mocked payments and module presence are not production acceptance. See [SaaS product journey](saas-product-journey.md).

| Batch | Delivered | Remaining boundaries |
| --- | --- | --- |
| P0-A Customer application shell | Public/account/workspace routes and layouts, marketing/pricing, account and organization onboarding, preserved signup destination | Public pages remain client rendered; prerendering and static SEO HTML are not accepted |
| P0-B Paid business features | Server-side command/query entitlements, lifetime organization report allowance, atomic reservations, idempotency, failure/cancellation refunds and upgrade UI | Not general credits or metered billing; real Stripe collection and complete crash reconciliation need separate acceptance |
| P1-A User files | Proxied upload progress/cancel, organization/user scoped list, download, delete and reference protection | Declarative record attachments, presigned initiation/completion/expiry, multipart and scanning are not implemented |
| P1-B Business dashboard and reference application | Team report studio connects marketing → signup/organization → upload → task → download → exhausted allowance → upgrade; metrics define source, window and freshness | Browser upgrade uses mocked APIs; visitor and settlement events remain disconnected and return unavailable metrics |

Keep marketing content and specific business UI in application-owned code; the framework owns
shared contracts and authorization. An AI example can be an optional extension of that journey,
without making the baseline demo depend on an external model. Versioned agent guidance and one
platform deployment walkthrough follow as separate work from a generic MCP adapter or hosted platform.

## 7. Current P0–P2 delivery (2026-11-05)

Implementation and regression entry points are in place. See [product capabilities](saas-product-capabilities.md) for contracts, configuration and external-acceptance boundaries.

| Priority | Implemented | Boundary |
| --- | --- | --- |
| P0 | Record attachments/replacement/detach, staged local/S3 initiation/completion/cancel/expiry, customer results/retry | Not declarative file fields, multipart or scanning; original library visibility remains; real S3 requires operator acceptance |
| P1 | Inbox/read state/preferences, account/TOTP/recovery, team transfer/removal and seat limits, monthly/purchased credits/refunds | Password-login MFA, not full OAuth MFA challenge; no generic metered/seat-priced subscription billing |
| P1 | Consented visits, signup/activation windows, D7 activation retention, signed payment captures/refunds and currency-separated net receipts | Not attributed conversion, MRR or bank settlement; local events do not certify real Stripe collection |
| P2 | Task board/calendar, unchanged snapshot/report/member approval history, optional credit-backed Ollama drafts | Application scenarios, not a general workflow engine; AI disabled by default; real service/output quality accepted separately |
| Journey | Demo marketing/pricing prerendering and SEO metadata; existing isolated Chromium production configuration/acceptance path | Not general SSR; production domain, credentials, platform and PDF require deployment acceptance |

Multipart/scanning, runtime global i18n, settlement/MRR, general cross-entity workflows, arbitrary AI providers, hosting and account deletion/export remain explicitly outside this batch.
