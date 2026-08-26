# Upgrading

AppStruct technical-preview releases use lockstep versions for the CLI, compiler, generated
runtime, official templates, and modules. Treat an upgrade as a source, generated-code, and
database change that must pass the same review as an application release.

## Project Upgrade Procedure

1. Commit or otherwise back up `appstruct.yaml`, `appstruct.lock`, `spec/`, `app/`,
   `migrations/`, and `.appstruct/schema.snapshot.json`.
2. Install the intended AppStruct CLI release and verify `appstruct --version`.
3. Run `appstruct update` in the application checkout.
4. Review the resulting `appstruct.lock`, `generated/`, API, and UI diffs.
5. Run the database checks below against an isolated PostgreSQL copy before production.

`appstruct update` acquires both update and generation locks, copies user-owned project inputs to
a staging workspace, writes the canonical candidate lock, compiles the complete App Spec,
regenerates all owned artifacts, runs release Clippy/build for Rust, builds the Web application,
and runs generated-backend release tests. It rechecks source hashes before committing. Only the
lock and owned generated tree are replaced, using a recoverable joint journal transaction.

Database verification remains explicit because update never connects to or changes a database:

```bash
appstruct check --deny-warnings
appstruct migrate plan
appstruct generate --check
appstruct build
appstruct migrate status
```

If `migrate plan` reports a destructive or manual-review change, stop. AppStruct intentionally
does not convert that plan into an automatically accepted migration. Write and review the required
migration as an application-owned release change.

## Current Boundaries

The technical-preview updater canonicalizes locks for presets supported by the installed CLI. It
does not rewrite App Spec syntax, merge a newer one-time Template into user files, edit migrations,
or select an unsupported future preset version. A release that requires a semantic Spec change
must provide reviewed manual steps before `appstruct update` can succeed.

Stale AppStruct versions and preset digests in a readable lock can be repaired. Invalid lock TOML,
unsupported presets, changed source files during verification, modified generated files, build
failures, and test failures all stop the update before commit.

## What Is Preserved

- `app/`, `spec/`, migrations, schema snapshots, environment files, and Template-owned files are
  never overwritten by update.
- `generated/` is framework-owned and may be replaced only after ownership hashes are validated.
- an edited or unknown file under `generated/` causes update to fail closed.
- migration IDs and checksums are immutable after application.
- the schema snapshot describes the latest accepted migration target and travels with migrations.

An interrupted update is recovered by the next `appstruct update`. Normal generation refuses to
run while update staging, backup, or journal state remains. Do not manually delete those paths
while investigating an ambiguous recovery error. Deleting `.appstruct/cache/` is safe and affects
speed only.

## Rollback

Roll back the application and CLI binaries to their previous versions only if the database is
still compatible. AppStruct does not generate down migrations and does not automatically undo an
applied schema change. Use a separately reviewed forward repair or database restore when needed.

Keep the previous backend binary and Web artifact until the new release passes health and
user-journey checks. A generated backend should not start against migration history from a newer
incompatible release.
