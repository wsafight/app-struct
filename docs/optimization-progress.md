# Delivery Optimization

Sequential work authorized on 2026-09-05. Each step includes its public contracts and relevant
verification before the next step starts.

- [x] Scalar correctness: bigint/decimal transport, shared Web codecs, local datetime round trips.
  Runtime tests (23), generated scalar contract test, four-timezone codec tests, generated SaaS Web
  tests (32), lint, production build and scoped Clippy passed.
- [x] Authorization: cross-module scope invariants and executable regression coverage.
  2,112 PostgreSQL truth-table comparisons cover list, member, trash and relation scopes. Fixed
  soft-deleted relation targets remaining queryable. Operations API and desktop/mobile browser
  checks passed with additional list, aggregate, CSV, Workflow, Activity and download isolation.
- [x] Custom screens: shared form and URL controllers, explicit relation labels and navigation.
  Generated Web tests (35), formatting, lint, TypeScript and production build passed, together with
  compiler/IR contracts, generated Rust compilation and Operations API/browser checks.
- [x] Aggregate editing: accepted parent/child contract, atomic API, generated editor and tests.
  Generated Rust compilation, compiler validation, PostgreSQL rollback/authorization/concurrency,
  desktop/mobile editing, conflict draft retention and template quality checks passed.
- [x] Production reports: isolated renderer adapter, packaged fonts/assets and lifecycle checks.
  Chromium service, typed renderer option, generated deployment assets, leases and transactional
  publication implemented. PDF and PostgreSQL lease renewal/cancellation/stale publication checks
  passed. Linux container
  isolation must still run in CI because Docker is unavailable on this host.
- [x] Operations: bounded metrics and database workload benchmarks.
  Generated backend compilation, HTTP label isolation and job success/cancellation/lease-loss
  metrics passed. The 20,000-row, two-tenant, eight-client PostgreSQL API workload passed with zero
  errors; debug p95 was 42/9/7/8/27 ms for offset/cursor/count/read/audited CRUD respectively.
- [x] Delivery: installable release artifacts and repeatable first-deployment verification.
  Installers, same-origin production API routing, Linux image builds, health probes and persistent
  files implemented. Native release/desktop/mobile acceptance and atomic installer/checksum failure
  checks passed. Fixed module-disabled Web builds and mobile table overflow. All three Web templates
  pass formatting/lint/types/tests/build; all eight Rust packages pass packaging verification.
  Linux container isolation/deployment and Windows installer acceptance remain CI requirements.

Public publishing is a separate release action after the local artifacts and checks are reviewable.

Final local gates: 371 workspace Rust tests, rustfmt, Clippy with warnings denied, eight crate
package verifications, and cargo-deny 0.20.2 advisories passed. Workspace tests use the CI performance
multiplier of two; the isolated compiler/generator performance test also passed its default budget.
Operations additionally passes 36 Web tests, lint, formatting, TypeScript, production build, and
PostgreSQL plus desktop/mobile workflow regression. The local preview uses an external disposable
test database and Capture rendering; it is not a Linux Chromium deployment acceptance result.

## 2026-10-12 Product Workflows

- [x] List projections include readable currency, owner and action dependencies; money CSV exports retain currency.
- [x] CSV import adds mapping, editable previews of 25 rows per page, type validation, row results and failed-row retries; the backend parses JSON cells as structured values.
- [x] Advanced filters support nested AND/OR editing and condition errors; facet buckets support clickable filters and null values.
- [x] Bulk updates reuse form controls and exact scalar validation, review values and revisions before submission, and show per-record results with failed-record retries.
- [x] Database pull previews use actual resource paths and provide next steps; doctor checks pending migrations, with a first-run guide.
- [x] Reports support UTC day/week/month groups, saved-view configuration and drilldown links; values that cannot be charted precisely retain their exact table representation.

Management lists retain page pagination and exact totals; report changes preserve the current list page. At that time, existing databases required manual schema and migration-history alignment. Explicit `migrate baseline` was added on 2026-10-27; see the update below and the [existing database guide](existing-database.md).

Validation passed: 463 workspace Rust tests, 55 generated Web tests, rustfmt, Clippy with warnings denied, TypeScript, ESLint, Prettier, production build and generated-tree snapshots. Real PostgreSQL query regression covers JSON import retries, UTC year boundaries, tenant/field authorization, page totals, cursors, filters and exports, with no migration drift. Dashboard initialization and read-only database pull were also verified.

## 2026-10-12 Generated Admin UI

Unified the light sidebar, green primary actions, color tokens, spacing, corner radii and typography. Resource navigation gains icons, groups and current-page breadcrumbs, with account and sign-out controls at the bottom of the sidebar. Search, filters, saved views and column options share a control panel; reports have their own section.

Tables gain hover and selection states, enum/boolean badges and wider action columns. Forms, authentication, charts, CSV previews and dialogs share the visual system; failure reasons wrap inside dialogs. Mobile layouts retain local table scrolling, horizontal navigation and single-column forms, with reduced indentation for deep filter groups. Focus indicators and reduced-motion preferences remain supported. Lists retain page pagination and exact totals.

Validation: minimal/dashboard/saas generated Web projects passed 51/54/55 tests respectively, TypeScript, ESLint, Prettier and production builds. Eighteen generation contract tests, three generated-tree snapshot tests, rustfmt and Clippy with warnings denied passed. No browser is available in this session, so desktop/mobile screenshot acceptance was not performed.

## 2026-10-13 Tenant switching and browser contract hardening

- [x] Tenant changes reset active React Query resources before refetching, so an organization cannot
  display another organization's cached list, audit, or report data while the new request is pending.
- [x] Updated tenant, audit, and SaaS browser contracts for the custom organization dialog, scoped
  resource form controls, current empty-state text, and audit snapshot duplication.
- [x] Added generated Web tests for both tenant selection and organization creation, including a
  late response from the previous tenant and preservation of the authenticated session cache.

Validation: Tenant, Audit, SaaS, Mail, Jobs/Schedules/Webhooks/Realtime, File, Bulk, and Chromium
report PostgreSQL/browser E2E suites passed. The SaaS generated template passed 57 Web tests,
TypeScript, ESLint, Prettier, production build, and generated-tree snapshots.

## 2026-10-13 Tenant-scoped query namespaces

- [x] Resource, audit, activity, and report query keys now include the selected tenant scope.
- [x] Realtime resource and activity subscriptions rebind when the tenant changes, so an old
  organization cannot keep an active event stream after switching contexts.
- [x] Added generated Web coverage for distinct cache keys across organizations while preserving
  the existing immediate reset behavior during a switch.

Validation: generated Web framework tests, TypeScript, ESLint, Prettier, production builds, and
generated-tree snapshots pass for the affected templates.

## 2026-10-27 Product journeys and current formats

- [x] [Existing database adoption](existing-database.md): `migrate baseline` previews without writes; `--accept` records only a matching canonical initial migration without seeds, without executing its CREATE SQL. Drift, custom SQL, migration chains and dirty history block adoption. Schema repair still requires review.
- [x] [Authentication limits](authentication-limits.md): registration is limited before password hashing, and verification issuance/resends share a counter. PostgreSQL cross-instance counters and `429 / Retry-After: 60` are verified. Trusted deployment gateways define source limits.
- [x] [Branding and themes](web-customization.md): all templates provide user-owned `app/web/setup.ts` for title, logo, palettes, themes and common messages, preserved by generate/update. This is partial text customization, not complete i18n.
- [x] [Data lifecycle](data-lifecycle.md): audit redaction before insertion, optional retention and explicit batched pruning; file references, retention collection, builtin-reference protection and storage-failure retries. Metadata row locks span object deletion to prevent concurrent collectors deleting a reused key. Storage deletion cannot roll back with the database; storage orphans without metadata still need reconciliation.
- [x] [Business extensions](business-extensions.md): the runnable example covers Policy rejection, transactional Hook rollback, after-commit failure and request-ID logs. The [upgrade guide](upgrading.md) covers read-only `template diff` and manual merge verification.
- [x] [Frontend validation](headless-controller.md): generated Web no longer directly depends on Zod; TanStack Form calls `valueError`, retaining UUID, exact scalars, amount/currency pairs, conversion and conflict drafts. Unrelated tooling transitive dependencies remain.
- [x] [Current format contracts](compatibility.md): only IR 17, snapshot 3 and layout 2 are read/written. Removed historical defaults, aliases and layout inference; kept version matching, ownership hashes, migration history/checksums and crash recovery. Back up old development directories and recreate them with the current template.
- [x] [Journey records](product-journeys.md): optional local stage/timing/status/help/reason records store no command arguments, environment or output and upload nothing. Recorder regression runs in CI.

### Actual local verification

- minimal/dashboard/saas passed 57/60/63 Web tests respectively, types, lint, formatting and production builds. Frozen installs in separate directories reused the local pnpm store; network-only cold installation was not verified.
- Actual `appstruct build`, `deploy verify`, native release backend and production Web smoke plus desktop/mobile revision-safe CRUD passed. Browser checks verified the user setup title, dark theme, palette and form messages.
- Isolated PostgreSQL checks retained existing rows through a second nullable-field migration and actual `pg_dump/pg_restore`. Restored status was applied 2 / pending 0 / drift none. The dedicated cluster was stopped and removed.
- CLI, Contracts, IR, Schema, Compiler and Codegen regressions, the full real-PostgreSQL Migrate suite, lifecycle/extensions/concurrent collection, generated-tree goldens and scoped Clippy passed. Current composition-root and local/S3 file artifacts compiled.
- macOS local archive installation, replacement, corrupt-archive rejection and original-binary preservation passed, as did recorder regression, rustfmt, documentation builds and link checks.

### Awaiting execution in the relevant environment

- [ ] Linux Docker/Compose first deployment and container isolation results for this change.
- [ ] Windows x64 installer results for this change. Full CI now includes the gate without requiring a version tag.
- [ ] Consenting real-user journey observations; no adoption, willingness-to-pay or retention conclusions are available.

See [quality gates](continuous-integration.md) for platform execution and status, and the [product roadmap](next-product-roadmap.md) for conditional future investment. Tests and workflow definitions do not establish platform results or real-user feedback.

## 2026-10-28 six product improvements and the 0.2.0 candidate

- [x] [Database adoption preflight](existing-database.md): compile an include proposal in memory and compare the actual schema read-only, reporting missing revision columns, omitted tables and diagnostics. Structural alignment does not accept migration history.
- [x] [CSV result recovery](bulk-operations.md): immutable UUID batches, content digests and saved results commit with business data. Concurrent retries write once; response loss can be resolved by querying the result. Changed content is rejected and resource/actor/tenant scopes remain isolated.
- [x] [Editing conflict merge](headless-controller.md): compare baseline, server and draft, select competing changes, then explicitly save using the compared revision. Datetime comparisons and selections retain microseconds.
- [x] [Permission preview](access-preview.md): explain CRUD, fields, owner, tenant and aggregate-child restrictions; custom Rust Policy is explicitly not evaluated.
- [x] [Chinese UI and formatting](web-customization.md): startup locale and field-label configuration, standard copy and shared numeric, monetary, date and display-zone formatting. Native datetime controls accept millisecond displays while unchanged values retain original microseconds; button widths and status styles are corrected.
- [x] [Conditional and cross-field validation](form-validation.md): typed `required_when` and strict `greater_than`, shared frontend `valueError`, and server validation of complete candidates after Hooks across partial patches, CSV, bulk, aggregates and Workflow. Explicit null clears nullable fields. No direct Zod dependency was restored; new forms submit untouched required false checkboxes.

Version metadata is prepared for 0.2.0 with IR 18 / runtime API 5 / snapshot 3 / layout 2. [Candidate release notes](release-0.2.0.md) explain the migration boundary. A crates.io check confirmed that 0.1.0 was published on 2026-08-29; the assumption of no previous release is incorrect. 0.2.0 introduces breaking format changes. This preparation has not pushed, tagged, published crates or created a Release.

### Local verification

- minimal/dashboard/saas passed 64/67/70 Web tests respectively, frozen installation, production dependency audit, Prettier, ESLint, TypeScript and production builds. Installation reused the local pnpm store.
- Chromium desktop and 390px mobile checks passed against the actual generated UI: Chinese copy and field labels, microsecond timestamps in native controls, conflict comparison/explicit save, CSV result recovery after response loss, and creation with an untouched required false checkbox and a zero count. Datetime codecs also passed in UTC, New York, Berlin and Shanghai. HTTP responses were intercepted in this UI check; separate real PostgreSQL integration tests verify server transactions, concurrency and isolation.
- All ten official crates passed actual packaging and verification builds. The macOS arm64 release archive, checksum, installation into a path with spaces, replacement, corrupt-archive rejection and original-binary preservation passed.
- Workspace Clippy with warnings denied, rustfmt, cargo-deny advisories/licenses/sources, journey recorder, actionlint and all shell syntax checks passed. Advisory fixes include ctutils 0.4.4, Rustls 0.23.46, rustls-webpki 0.103.16, webpki-root-certs 1.0.10 and Tokio 1.53.3.
- The documentation site check reported no errors/warnings; 54 pages, search and internal links built successfully.
- 473 workspace Rust tests passed with zero failures or ignored tests, including real PostgreSQL migration, adoption, batch and form-rule integration. Performance checks used the CI multiplier of 2; the latest generated-tree golden tests also passed separately.

The dedicated PostgreSQL cluster and browser preview were stopped and temporary projects removed. Logs, screenshots and the native candidate archive remain under the ignored `target/` directory.

Linux Docker/Compose, renderer isolation, the Windows installer and the full five-platform release matrix still require actual runs on the candidate commit. Voluntary real-user observation has not run either. Local results do not count as acceptance in those environments.
