#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
  echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
  exit 2
fi

workspace="$(cd "$(dirname "$0")/.." && pwd)"
api_port="${APPSTRUCT_E2E_API_PORT:-33600}"
web_port="${APPSTRUCT_E2E_WEB_PORT:-55600}"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-m6-jobs.XXXXXX")"
project="$temporary_root/project"
log="$temporary_root/dev.log"
dev_pid=""

cleanup() {
  if [[ -n "$dev_pid" ]] && kill -0 "$dev_pid" 2>/dev/null; then
    kill -INT -- "-$dev_pid" 2>/dev/null || true
    wait "$dev_pid" 2>/dev/null || true
  fi
  case "$temporary_root" in
    "${TMPDIR:-/tmp}"/appstruct-m6-jobs.*) rm -r "$temporary_root" ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$project"
cp -R "$workspace/tests/fixtures/m6-jobs-project/." "$project/"
cd "$workspace"
cargo build --locked -p appstruct-cli

set -m
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
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
jar="$temporary_root/jobs.cookies"
email="jobs-$RANDOM-$$@example.com"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$email\",\"password\":\"correct-horse-battery\"}" \
  "$api/api/auth/register" >"$temporary_root/user.json"
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$jar")"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"Jobs Tenant"}' "$api/api/tenant/organizations" \
  >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"

DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" TENANT_ID="$tenant" \
  cargo run --quiet --manifest-path "$project/app/jobs-e2e/Cargo.toml"
pnpm --dir "$project/generated/web" exec tsc --noEmit

kill -INT -- "-$dev_pid"
wait "$dev_pid"
dev_pid=""
grep -q "shutdown signal received" "$log"
grep -q "job worker stopped" "$log"

backend="$project/.appstruct/cache/backend-target/debug/appstruct-generated-backend"
if APPSTRUCT_ENV=production DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  APPSTRUCT_BIND="127.0.0.1:0" "$backend" >"$temporary_root/config-error.log" 2>&1
then
  echo "invalid production mail configuration unexpectedly started" >&2
  exit 1
fi
grep -q "capture mail provider is forbidden in production" "$temporary_root/config-error.log"
if grep -q "panicked at" "$temporary_root/config-error.log"; then
  cat "$temporary_root/config-error.log" >&2
  exit 1
fi
echo "Jobs PostgreSQL transaction and worker E2E passed"
