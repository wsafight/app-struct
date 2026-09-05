#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "$0")/.." && pwd)"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-template-build.XXXXXX")"

cleanup() {
  case "$temporary_root" in
    "${TMPDIR:-/tmp}"/appstruct-template-build.*) rm -r "$temporary_root" ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}
trap cleanup EXIT

cargo build --locked -p appstruct-cli
target/debug/appstruct --project "$temporary_root" new ci-check --template "${APPSTRUCT_TEMPLATE:-saas}"
target/debug/appstruct --project "$temporary_root/ci-check" generate

cd "$temporary_root/ci-check/generated/web"
pnpm install --frozen-lockfile
pnpm audit --prod --audit-level high
pnpm run format:check
pnpm run lint
pnpm run typecheck
pnpm run test
pnpm run build
