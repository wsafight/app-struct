# 0.2.0 release candidate

Status: prepared locally, not published. Date: 2026-10-28.

This candidate improves the path from existing PostgreSQL to daily admin work:

- Read-only `db pull` adoption preflight, followed by explicit schema reconciliation and baseline acceptance.
- Transactional CSV batch idempotency and committed-result recovery across response loss.
- Editing conflicts compare the baseline, server and draft; choices are applied before an explicit save with the compared revision.
- `access preview` explains declared resource/field/owner/tenant decisions without executing custom Policy.
- Startup English/Chinese UI, field-label overrides and shared exact numeric/date/timezone display.
- Typed `required_when` and `greater_than` rules shared by standard Web forms and final server candidates; explicit null patches now clear nullable fields correctly.
- Dependency advisory fixes: generated backends and the workspace use Tokio 1.53.3, with fixed TLS and constant-time dependencies in the lockfile.

The candidate also includes the previous authentication limits, branding/theme, data lifecycle, extension example, template diff and direct frontend Zod removal. All official crates and vendored runtime metadata use 0.2.0. Current contracts are IR 18, runtime API 5, snapshot 3 and project layout 2.

0.1.0 was published on crates.io ([compiler record](https://crates.io/crates/appstruct-compiler/0.1.0), 2026-08-29); GitHub had no Releases or tags at this check. 0.2.0 includes breaking format changes and provides no historical IR, snapshot or layout adapters. Back up first, recompile IR from Spec, and review/recreate projects using old formats in a separate directory following [Upgrading](upgrading.md); normal update does not convert them. Existing databases need a reviewed migration for `_appstruct_import_batches` and a full comparison of other system tables, columns and indexes. Preserve data, applied migrations and checksums; do not run initial creation SQL against an existing database.

Local verification is recorded in [optimization progress](optimization-progress.md). Linux container/isolation, Windows installation and the full native release matrix require actual runs on the candidate commit; workflow definitions alone do not satisfy these gates. Runtime language switching, arbitrary cross-record rules and automatic import-ledger expiration are outside this candidate.

Run full CI on the reviewed commit before tagging `v0.2.0`, following [release instructions](releasing.md). Tags, remote writes, crate publication and public Releases are separate maintainer actions. No tag or Release has been created by this preparation.
