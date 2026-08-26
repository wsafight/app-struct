# AppStruct

AppStruct is a configuration-driven Rust full-stack application generator. It compiles a
multi-file YAML App Spec into a typed IR, PostgreSQL migrations, an Axum/SeaORM backend,
OpenAPI, a TypeScript client, and a React/Vite application.

The repository is currently a technical preview. M0-M5 are complete, including templates,
production builds, environment diagnostics, and the coordinated development server. The M6
Tenant, Audit, and Mail modules are available as previews; Jobs, File, and the SaaS preset remain
in development.

## Quick Start

The workspace pins Rust 1.98.0. Install the CLI from this checkout:

```bash
rustup toolchain install 1.98.0 --component clippy,rustfmt
cargo build --release --locked -p appstruct-cli
export PATH="$PWD/target/release:$PATH"
appstruct --version
```

Create a minimal project backed by an existing PostgreSQL database:

```bash
appstruct new notes --template minimal
cd notes
cp .env.example .env
appstruct doctor
appstruct dev
```

Update `DATABASE_URL` in `.env` before running `doctor`. For a Docker-managed PostgreSQL
development database, use `--template dashboard` and ensure Docker Compose is available.

`appstruct dev` starts the generated API and Vite, applies only safe development
migrations, watches App Spec and user Rust inputs, and stops its child processes on Ctrl-C.
The default URLs are `http://127.0.0.1:3000` and `http://127.0.0.1:5173`.

Enable tenant isolation together with Auth and mark each tenant-owned entity explicitly:

```yaml
modules:
  auth:
    enabled: true
    user_entity: User
  tenant:
    enabled: true

entities:
  Project:
    tenant: true
```

The generated Web application provides organization onboarding and switching. Generated clients
send `X-AppStruct-Tenant`; the backend validates membership and injects `tenant_id` into every
tenant-scoped CRUD query and write.

Enable Audit with Auth, choose the reader roles, and opt entities into transactional snapshots:

```yaml
modules:
  audit:
    enabled: true
    reader_roles: [admin]

entities:
  Project:
    audit: true
```

Create, update, and delete snapshots are committed in the same PostgreSQL transaction as the
business write. The generated Audit page and read-only API enforce reader roles and current-tenant
isolation.

Mail supports compile-time validated templates and `capture`, SMTP, or Resend providers. Provider
credentials remain in server environment variables, and business handlers can call the generated
`RequestContext::send_mail` capability. Capture is development-only and rejected in production.

## Commands

```text
appstruct new <name> --template minimal|dashboard
appstruct check [--format text|json]
appstruct generate [--check]
appstruct migrate plan|dev|apply|status
appstruct dev [--api-port <port>] [--web-port <port>]
appstruct build
appstruct doctor [--format text|json]
```

`migrate plan` is read-only. `migrate dev --accept` creates and optionally applies only
non-destructive online migrations. Production deployment uses `migrate status` followed by
the explicit `migrate apply` command.

## Documentation

- [Installation](docs/installation.md)
- [Upgrading](docs/upgrading.md)
- [Deployment](docs/deployment.md)
- [Product requirements](PRODUCT.md)
- [Technical design](TECHNICAL_DESIGN.md)

The `references/` directory contains local research material and is intentionally excluded
from version control and product commits.

## Development

Run the repository quality gates with the pinned local toolchain:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run the PostgreSQL browser gate against a dedicated test database:

```bash
APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_e2e' \
  scripts/run-m5-browser-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_tenant_e2e' \
  scripts/run-m6-tenant-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_audit_e2e' \
  scripts/run-m6-audit-e2e.sh

APPSTRUCT_E2E_DATABASE_URL='postgresql://user:password@127.0.0.1/appstruct_mail_e2e' \
  scripts/run-m6-mail-e2e.sh
```

Rust source files are limited to 400 lines by a repository test. Generated projects also pin
their Rust and pnpm dependency graphs so repeated generation and production builds remain
reviewable and reproducible.

## License

Workspace crates are licensed under MIT OR Apache-2.0.
