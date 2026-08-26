# Upgrading

AppStruct technical-preview releases use lockstep versions for the CLI, compiler, generated
runtime, official templates, and modules. Treat an upgrade as a source, generated-code, and
database change that must pass the same review as an application release.

## Current Limitation

The designed `appstruct update` transaction is not implemented yet. Do not invoke it or edit
an applied migration to simulate an upgrade. Until the command exists, upgrades use an
explicit staging checkout and the process below.

## Project Upgrade Procedure

1. Commit or otherwise back up `appstruct.yaml`, `appstruct.lock`, `spec/`, `app/`,
   `migrations/`, and `.appstruct/schema.snapshot.json`.
2. Create a separate staging checkout of the application. Do not test an upgrade directly
   against the production database.
3. Switch the AppStruct source checkout to the intended revision, run
   `cargo build --release --locked -p appstruct-cli` from its workspace root, and install the
   resulting `target/release/appstruct` binary.
4. Update the AppStruct version in the staging project's `appstruct.lock` only when the
   candidate release requires it.
5. Run the verification sequence below against an isolated PostgreSQL copy.
6. Review Spec, migration, generated manifest, API, and UI diffs before merging the staging
   changes into the application branch.

Verification sequence:

```bash
appstruct check
appstruct migrate plan
appstruct generate
appstruct generate --check
appstruct build
appstruct migrate status
```

If `migrate plan` reports a destructive or manual-review change, stop. AppStruct intentionally
does not convert that plan into an automatically accepted migration. Write and review the
required migration as an application-owned release change.

## What Is Preserved

- `app/` and template-owned files are user code and are never overwritten by generation.
- `generated/` is framework-owned and may be replaced after ownership hashes are validated.
- an edited or unknown file under `generated/` causes generation to fail closed.
- migration IDs and checksums are immutable after application.
- the schema snapshot describes the latest accepted migration target and must travel with the
  migration directory.

Deleting `.appstruct/cache/` is safe; it affects speed only. Do not delete generation journals,
backups, or staging directories while a recovery error is being investigated.

## Rollback

Roll back the application and CLI binaries to their previous versions only if the database is
still compatible. AppStruct does not generate down migrations and does not automatically undo
an applied schema change. For a database rollback, use a separately reviewed forward repair or
restore procedure appropriate to the production environment.

Keep the previous backend binary and Web artifact until the new release passes health and
user-journey checks. A generated backend should not be started against migration history from
a newer incompatible release.
