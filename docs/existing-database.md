# Existing PostgreSQL to the first running application

`db pull` reads catalog metadata and writes a reviewable domain draft. It preserves table and column names, previews the actual resource and API paths, and prints next commands. It does not add the draft to `includes`, change the source database, or adopt its migration baseline.

1. Create a project with `appstruct init imported-admin --template dashboard --database-mode external`. Set the source `DATABASE_URL` in its `.env`.
2. Run `appstruct db pull --schema public --output spec/imported.yaml --review`. Review tables, warnings and access rules. Scripts can choose `--access authenticated` or `--access role --role admin` with the corresponding authentication/RBAC modules enabled. The default omits access rules and fails closed until they are supplied.
3. Resolve unsupported keys/types/relations and table conflicts with template examples. Add the reviewed file to `appstruct.yaml` includes. Review AppStruct's required `revision` column and generated module tables.
4. Run `appstruct check`, `appstruct generate`, `appstruct migrate plan` and `appstruct migrate lint`. An existing schema is not automatically registered as a migration baseline: do not apply an initial CREATE TABLE plan to the same existing tables.
5. For a first runnable demonstration, point `.env` at a fresh development database. Run `appstruct migrate dev --accept`, `appstruct migrate apply`, `appstruct doctor` and `appstruct dev`, then import sample data through CSV. Using the original database requires aligning its schema and migration history first.
6. Register a development account and verify allowed reads and rejected writes. Check roles, owner fields and tenant context where applicable. A table called `tasks` is served at `/tasks` and `/api/tasks/`, as the pull preview shows.

Doctor reports pending migrations as a failed readiness check and gives the next command. A missing connection URL, unsupported database shape, or missing access rule should be resolved before starting the app. Environment variables override `.env` values.

Resource lists retain page pagination and exact totals. CSV import has mapping, paged preview, validation and failed-row retry. Filters support nested groups and clickable counts. Summary settings can be saved with private or team views and link back to matching records.
