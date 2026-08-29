# Installation

AppStruct is currently installed from a source checkout. The repository is prepared to publish
crates.io packages and checksummed macOS/Linux archives, but no public package or binary installer
is assumed to exist during the technical preview.

## Requirements

| Dependency | Required version | Used for |
| --- | --- | --- |
| Rust | 1.98.0 with rustfmt and Clippy | CLI and generated backend |
| PostgreSQL | 17 recommended | migrations and generated API |
| Node.js | 24 recommended | generated Web application |
| pnpm | 9.12.3 | locked Web dependency installation |
| Docker Compose | Current release, optional | `database.dev.mode: managed` only |

The root `rust-toolchain.toml` and every generated project pin Rust 1.98.0. This was the
local latest Rust version used for the current implementation baseline.

## Install The CLI

From the AppStruct repository root:

```bash
rustup toolchain install 1.98.0 --component clippy,rustfmt
cargo build --release --locked -p appstruct-cli
./target/release/appstruct --version
```

Building from the workspace root is intentional: it makes Cargo honor the committed root
`Cargo.lock`. `cargo install --path crates/appstruct-cli --locked` does not use that workspace
lock in the current source layout and is therefore not the reproducible installation path.

Either add `target/release` from this checkout to `PATH`, or install the built binary into an
existing user-owned directory on `PATH`. For example, on macOS or Linux:

```bash
install -d "$HOME/.local/bin"
install -m 0755 target/release/appstruct "$HOME/.local/bin/appstruct"
```

Confirm that the installed binary and required tools are visible:

```bash
appstruct --version
rustc --version
pnpm --version
```

Re-run the locked release build and replace the installed binary after switching this checkout
to another AppStruct revision.

When a release provides a platform archive, download both the `.tar.gz` and matching `.sha256`
file into one directory, verify it, and install the contained binary:

```bash
shasum -a 256 -c appstruct-<version>-<target>.tar.gz.sha256
tar -xzf appstruct-<version>-<target>.tar.gz
install -m 0755 appstruct-<version>-<target>/appstruct "$HOME/.local/bin/appstruct"
```

Linux users may use `sha256sum -c` instead. Do not install an archive when its checksum fails.
After the crates are published, `cargo install appstruct-cli --version <version> --locked` is the
registry equivalent; source and binary release versions remain lockstep.

## Start With External PostgreSQL

The `minimal` template expects an existing database:

```bash
appstruct new notes --template minimal
cd notes
cp .env.example .env
```

Create the database using your normal PostgreSQL administration workflow, then edit `.env`
so `DATABASE_URL` contains the correct user, password, host, port, and database. The example
URL disables TLS for local development only.

Validate the environment and start the application:

```bash
appstruct doctor
appstruct dev
```

The CLI applies safe initial migrations, generates and builds the backend, installs Web
dependencies with the committed pnpm lock, and starts both services. Override the defaults
when necessary:

```bash
appstruct dev --api-port 3100 --web-port 5200
```

The ports must be non-zero and different. External mode never starts or stops PostgreSQL.

## Start With Managed PostgreSQL

The `dashboard` template includes `compose.yaml` and defaults to managed mode:

```bash
appstruct new project-hub --template dashboard
cd project-hub
appstruct doctor
appstruct dev
```

Managed mode starts only the Compose `postgres` service. On Ctrl-C it stops that service only
when the current dev session started it, and it preserves the named database volume. If the
service was already running, AppStruct leaves it running.

Copy `.env.example` to `.env` only when overriding managed defaults. Process environment
variables take precedence over values in `.env`; secrets are never written into generated
artifacts or command output.

## Start From The SaaS Preset

The `saas` template also uses managed PostgreSQL and locks `appstruct/saas@1`:

```bash
appstruct new saas-demo --template saas
cd saas-demo
appstruct preset show
appstruct doctor
appstruct dev
```

After registration, create an organization and use the generated Project and Task resources. Both
are tenant-isolated and audited. Development defaults to capture mail and local files under
`.appstruct/files`; Jobs/Outbox uses PostgreSQL. Inspect the effective module configuration with
`appstruct preset show --expanded`.

New registrations use the `member` role. After the first operator registers, provision that
account exactly once from a trusted host:

```bash
appstruct auth bootstrap-admin --email admin@example.com
```

For production, replace capture/local providers through reviewed App Spec overrides and runtime
environment variables. Keep the generated preset digest and exact module versions in
`appstruct.lock`. The Admin operations overview is read-only in this preview; Billing is not part of
preset version 1.

## Troubleshooting

Export the bundled Draft 2020-12 schema for editor integration, and use structured diagnostics
or strict warnings in CI:

```bash
appstruct schema > appstruct.schema.json
appstruct doctor --format json
appstruct check --format json
appstruct check --deny-warnings
```

Common failures are:

- `DATABASE_URL` is absent or the external database is unreachable.
- Docker or Compose is unavailable for a managed project.
- the installed Rust or pnpm version differs from the project pins.
- API or Web ports are already in use.
- an applied migration checksum or the live PostgreSQL schema has drifted.

Run `appstruct migrate status` for migration history and drift details. AppStruct does not
repair checksum, dirty-history, or catalog drift automatically.
