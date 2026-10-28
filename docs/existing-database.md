# Existing PostgreSQL to the first running application

`db pull` reads catalog metadata and writes a reviewable domain draft. It preserves table and column names, previews the actual resource and API paths, and prints next commands. It does not add the draft to `includes`, change the source database, or adopt its migration baseline.

1. Create a project with `appstruct init imported-admin --template dashboard --database-mode external`. Set the source `DATABASE_URL` in its `.env`.
2. Run `appstruct db pull --schema public --output spec/imported.yaml --review`. Review tables, warnings and access rules. Scripts can choose `--access authenticated` or `--access role --role admin` with the corresponding authentication/RBAC modules enabled. The default omits access rules and fails closed until they are supplied.
3. Resolve unsupported keys/types/relations and table conflicts with template examples. Add the reviewed file to `appstruct.yaml` includes. Review AppStruct's required `revision` column and generated module tables.
4. Run `appstruct check`, `appstruct generate`, `appstruct migrate plan` and `appstruct migrate lint`. An existing schema is not automatically registered as a migration baseline: do not apply an initial CREATE TABLE plan to the same existing tables.
5. For a first runnable demonstration, point `.env` at a fresh development database. Run `appstruct migrate dev --accept`, `appstruct migrate apply`, `appstruct doctor` and `appstruct dev`, then import sample data through CSV. For existing data, use the explicit baseline workflow below after aligning the structure.
6. Register a development account and verify allowed reads and rejected writes. Check roles, owner fields and tenant context where applicable. A table called `tasks` is served at `/tasks` and `/api/tasks/`, as the pull preview shows.

## Read-only adoption preflight

`db pull` now returns `preflight` in normal JSON output and prints it before the final `--review` write confirmation. It compiles the proposed include in memory and compares the source schema in a read-only transaction. `blocked` includes omitted tables, missing revision columns, compiler diagnostics and structural differences; `structure_matches` means only the candidate structure matches. `history_checked: false` explicitly excludes migration history verification. The `include` proposal requires an explicit edit: no root configuration, migrations or data are changed. The preflight does not grant baseline acceptance.

## Adopt the existing database

Back up the database and rehearse against a copy. Review the initial SQL, including `revision` columns, module tables, defaults, constraints and indexes. Apply only the missing structure through your existing database change process; never run the full initial CREATE TABLE plan against existing tables. Baseline registration does not repair structure.

```bash
appstruct migrate baseline          # read-only comparison; differences block adoption
appstruct migrate baseline --accept # write migration/snapshot and register matching history
appstruct migrate status
appstruct doctor
appstruct dev
```

Preview creates no project files or database history. Acceptance records one initial migration without executing its SQL or changing business rows. The same migration can rebuild the structure in a fresh database. Repeated registration is idempotent. If files are committed but database registration fails, retain them, resolve the error and retry the same command.

All managed tables in the current PostgreSQL schema must match; extra tables, columns or indexes are not silently ignored. Only one unchanged canonical initial migration and its matching snapshot are accepted. Arbitrary SQL, existing migration chains and dirty history are rejected. Remove seeds from the baseline Spec and review them separately after adoption; registration must not silently mark unexecuted seed DML as applied. Constraints, triggers and RLS outside the Spec model still need separate review for compatibility with the generated app.

Verify that existing rows can be read and updated with revision checks. Add a nullable field and run `migrate dev --accept` and `migrate status` to validate a subsequent migration without losing existing data.

Doctor reports pending migrations as a failed readiness check and gives the next command. A missing connection URL, unsupported database shape, or missing access rule should be resolved before starting the app. Environment variables override `.env` values.

See [Authentication and email limits](authentication-limits.md) for registration and email retry behavior.

Resource lists retain page pagination and exact totals. CSV import has mapping, paged preview, validation and failed-row retry. Filters support nested groups and clickable counts. Summary settings can be saved with private or team views and link back to matching records.
