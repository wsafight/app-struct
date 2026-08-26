# Deployment

AppStruct produces a standalone Rust API binary, static Web assets, and explicit PostgreSQL
migrations. The CLI does not provision production infrastructure or automatically migrate a
database when the API starts.

## Build Artifacts

Set the public API URL before building because Vite embeds `VITE_API_URL` in the Web bundle:

```bash
export VITE_API_URL=https://api.example.com
appstruct check
appstruct build
appstruct generate --check
```

A successful build produces:

```text
.appstruct/cache/backend-target/release/appstruct-generated-backend
generated/web/dist/
migrations/
.appstruct/schema.snapshot.json
```

Ship the backend binary and Web `dist/` directory as immutable artifacts. The release job that
runs migrations also needs the matching AppStruct CLI, project root marker, migration files,
and schema snapshot. Do not build from a mutable branch inside the production runtime.

## Runtime Configuration

The backend reads configuration from its process environment:

| Variable | Requirement | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | required | PostgreSQL connection URL |
| `APPSTRUCT_BIND` | optional | listen address, default `127.0.0.1:3000` |
| `RUST_LOG` | optional | tracing filter |
| `APPSTRUCT_ENV` | set to `production` for Auth applications | enables production Auth defaults |
| `APPSTRUCT_ALLOWED_ORIGIN` | required for browser Auth deployments | exact allowed browser origin |
| `APPSTRUCT_FRONTEND_URL` | required for password-reset links | public Web origin |
| `APPSTRUCT_COOKIE_SECURE` | normally `true` in production | Secure attribute for Auth cookies |
| `APPSTRUCT_SESSION_TTL_HOURS` | optional | positive session lifetime, default 720 |
| `APPSTRUCT_AUTH_MAIL_MODE` | `smtp` when production password reset is enabled | Auth mail adapter |
| `APPSTRUCT_SMTP_HOST` | required for SMTP | SMTP relay host |
| `APPSTRUCT_SMTP_PORT` | optional | SMTP relay port |
| `APPSTRUCT_SMTP_USERNAME` | required for SMTP | SMTP credential |
| `APPSTRUCT_SMTP_PASSWORD` | required for SMTP | SMTP secret |
| `APPSTRUCT_SMTP_FROM` | required for SMTP | valid sender mailbox |

Inject secrets through the deployment platform. Do not bake `.env`, database credentials, or
SMTP credentials into an image or static Web assets. Terminate TLS at a trusted reverse proxy
or load balancer and use a TLS-protected PostgreSQL connection as required by the environment.

## Release Order

Run each release against one immutable artifact set:

1. Back up the database according to the service recovery policy.
2. Set the production `DATABASE_URL` without printing it.
3. Run `appstruct --project <release-root> migrate status`.
4. Run `appstruct --project <release-root> migrate apply` as a dedicated release job.
5. Start the new backend binary with its runtime environment.
6. Publish `generated/web/dist/` with SPA fallback to `index.html`.
7. Check `GET /health/live`, `GET /openapi.json`, login when enabled, and one authorized CRUD
   journey before retiring the previous release.

Migration apply uses an advisory lock, validates ordered history and checksums, and checks live
catalog drift after all pending migrations complete. A dirty non-transactional migration,
checksum mismatch, history gap, or drift blocks the release and requires investigation.

## Process And Network Model

Bind the backend to an internal interface such as `0.0.0.0:3000` only when the runtime network
requires it. Expose it through a reverse proxy that provides HTTPS, request size limits, access
logs, and deployment-level timeouts. Serve the Web directory from a static host or CDN and
route unknown application paths back to `index.html`.

The current generated backend exposes `/health/live`; it proves that the process can answer a
request but is not a database readiness guarantee. Keep the old release available until a
database-backed smoke journey succeeds. A dedicated readiness endpoint remains an explicit
MVP gate rather than something deployment tooling should infer from liveness.

## Failure And Rollback

If migration apply fails, do not start the new backend. Preserve logs and migration history,
then repair the failed release according to the reviewed migration procedure. Never modify the
checksum of a migration already recorded by a database.

If the application fails after a compatible migration, route traffic back to the previous
backend and Web artifacts. AppStruct does not provide automatic down migrations; incompatible
database rollback requires a reviewed forward repair or a database restore.
