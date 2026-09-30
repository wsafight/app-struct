# Generated-tree manifests

`<fixture>-generated.manifest` pins the complete `appstruct_codegen::plan()` output for one fixture
under `tests/fixtures/`. Each line is `<sha256>  <path>`, sorted by path. The test lives in
`crates/appstruct-codegen/tests/generated_tree_golden.rs`.

The manifests are hashes, not file contents, so a changed line tells you *which* generated file
changed but not *what* changed in it.

## Accepting a change

```bash
UPDATE_GOLDEN=1 cargo test -p appstruct-codegen --test generated_tree_golden
cargo test -p appstruct-codegen --test generated_tree_golden   # must pass on the second run
```

## Inspecting what changed

A failing run writes the freshly generated tree to `target/golden-actual/<fixture>/`, which is
gitignored. Diff it against the previous commit's tree:

```bash
cargo test -p appstruct-codegen --test generated_tree_golden   # fails, writes target/golden-actual/
git stash && cargo test -p appstruct-codegen --test generated_tree_golden 2>/dev/null || true
git stash pop
diff -ru target/golden-actual/<fixture> <somewhere>
```

In practice the cheaper path is to read the failure summary — it lists added / removed / changed
paths — and open the changed file directly.

## What is covered

| Fixture | Surface |
| --- | --- |
| `m0-project` | auth + rbac, owner/role access rules, relations |
| `m2-project` | baseline CRUD, enums, 1:N and 1:1 relations |
| `m3-project` | value objects, commands, queries, custom page and field components |
| `m6-preset-project` | `appstruct/saas@1` preset expansion |
| `m8-server-project` | `server.security_headers` |

## What is intentionally absent

These are produced outside `plan()` and so are never in a manifest:

- `generated/backend/Cargo.lock` and `generated/server/Cargo.lock` — build-generated and preserved
  across regenerations.
- `generated/.appstruct-manifest.json` — written by the CLI ownership transaction.

`web/pnpm-lock.yaml` and `web/.gitignore` are also excluded (`TEMPLATE_BACKED` in the test). Both are
byte-identical across every fixture and are verbatim copies of files under
`crates/appstruct-codegen/templates/web/`; hashing the lockfile would produce noise on every
dependency bump. The test asserts they still match their template instead.

## Known gap

Manifests hold pre-prettier bytes. `crates/appstruct-cli/src/generation/web_format.rs` reformats
`web/src/app/App.tsx`, `web/src/app/Layout.tsx`, `web/src/pages/ResourceDetail.tsx`, and every
`ArtifactKind::TypeScript` file after planning. Byte-exact on-disk equality is covered separately by
`crates/appstruct-cli/tests/cli.rs::generation_is_byte_deterministic_across_project_directories`.
