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

Management lists retain page pagination and exact totals; report changes preserve the current list page. Existing databases still require manual schema and migration-history alignment; automatic baseline adoption is unavailable.

Validation passed: 463 workspace Rust tests, 55 generated Web tests, rustfmt, Clippy with warnings denied, TypeScript, ESLint, Prettier, production build and generated-tree snapshots. Real PostgreSQL query regression covers JSON import retries, UTC year boundaries, tenant/field authorization, page totals, cursors, filters and exports, with no migration drift. Dashboard initialization and read-only database pull were also verified.
