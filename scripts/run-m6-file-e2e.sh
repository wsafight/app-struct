#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
  echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
  exit 2
fi

workspace="$(cd "$(dirname "$0")/.." && pwd)"
api_port="${APPSTRUCT_E2E_API_PORT:-33700}"
web_port="${APPSTRUCT_E2E_WEB_PORT:-55700}"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-m6-file.XXXXXX")"
project="$temporary_root/project"
storage="$temporary_root/storage"
log="$temporary_root/dev.log"
dev_pid=""

cleanup() {
  if [[ -n "$dev_pid" ]] && kill -0 "$dev_pid" 2>/dev/null; then
    kill -INT -- "-$dev_pid" 2>/dev/null || true
    wait "$dev_pid" 2>/dev/null || true
  fi
  case "$temporary_root" in
    "${TMPDIR:-/tmp}"/appstruct-m6-file.*) rm -r "$temporary_root" ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$project" "$storage"
cp -R "$workspace/tests/fixtures/m6-file-project/." "$project/"
cd "$workspace"
cargo build --locked -p appstruct-cli

set -m
APPSTRUCT_FILE_ROOT="$storage" DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  target/debug/appstruct --project "$project" dev --api-port "$api_port" --web-port "$web_port" \
  >"$log" 2>&1 &
dev_pid=$!
set +m

for ((attempt = 0; attempt < 180; attempt += 1)); do
  if curl --fail --silent --max-time 2 "http://127.0.0.1:$api_port/health/ready" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$dev_pid" 2>/dev/null; then
    sed -n '1,240p' "$log" >&2
    exit 1
  fi
  sleep 1
done
curl --fail --silent --max-time 2 "http://127.0.0.1:$api_port/health/ready" >/dev/null

api="http://127.0.0.1:$api_port"
jar="$temporary_root/file.cookies"
email="file-$RANDOM-$$@example.com"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$email\",\"password\":\"correct-horse-battery\"}" \
  "$api/api/auth/register" >"$temporary_root/user.json"
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$jar")"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"File Tenant"}' "$api/api/tenant/organizations" \
  >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"

APPSTRUCT_FILE_ROOT="$storage" DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" TENANT_ID="$tenant" \
  cargo run --quiet --manifest-path "$project/app/file-e2e/Cargo.toml"
pnpm --dir "$project/generated/web" exec tsc --noEmit
echo "File PostgreSQL and local object-storage E2E passed"
