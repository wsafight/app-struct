# Releasing AppStruct

The repository can build crates.io packages and checksummed macOS/Linux binaries. Publishing is a
maintainer action: credentials and the final repository URL are intentionally not stored here.

## Preflight

1. Set the workspace version once in `[workspace.package]`; all official crates inherit it.
2. Before the first public publish, add the canonical `repository` URL to workspace package
   metadata.
3. Confirm the worktree is clean and release notes describe compatibility and migration risks.
4. Run the pinned quality and packaging gates:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/verify-packages.sh
```

The packaging script uses local crates.io patches only while verifying the unpublished lockstep
crate set. Packaged manifests still contain normal registry dependencies with compatible versions,
not local paths.

## Publish Crates

Publish in dependency order and wait for each crate to appear in the crates.io index before its
consumers:

```text
appstruct-ir
appstruct-compiler and appstruct-migrate
appstruct-codegen
appstruct-cli
```

Use `cargo publish -p <package> --locked` for each package. The binary package name is
`appstruct-cli`, while the installed executable is `appstruct`. Publishing is not automated by the
binary release workflow so a source tag cannot consume crates.io credentials.

## Publish Binaries

Create and push a `v<workspace-version>` tag. `.github/workflows/release.yml` first runs formatting,
strict Clippy, and workspace tests, then builds these archives:

```text
x86_64-unknown-linux-gnu
aarch64-apple-darwin
x86_64-apple-darwin
```

Each archive contains `appstruct` and the root README, with a sibling `.sha256` file. The workflow
rejects a tag whose version does not match Cargo metadata and uploads artifacts only after the
quality job succeeds. Inspect the created GitHub release and test one fresh installation before
announcing it.
