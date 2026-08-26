#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
  echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
  exit 2
fi

workspace="$(cd "$(dirname "$0")/.." && pwd)"
api_port="${APPSTRUCT_E2E_API_PORT:-33300}"
web_port="${APPSTRUCT_E2E_WEB_PORT:-55300}"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-m6-tenant.XXXXXX")"
project="$temporary_root/project"
log="$temporary_root/dev.log"
dev_pid=""

cleanup() {
  if [[ -n "$dev_pid" ]] && kill -0 "$dev_pid" 2>/dev/null; then
    kill -INT -- "-$dev_pid" 2>/dev/null || true
    wait "$dev_pid" 2>/dev/null || true
  fi
  case "$temporary_root" in
    "${TMPDIR:-/tmp}"/appstruct-m6-tenant.*) rm -r "$temporary_root" ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$project"
cp -R "$workspace/tests/fixtures/m6-tenant-project/." "$project/"
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
first_jar="$temporary_root/first.cookies"
second_jar="$temporary_root/second.cookies"

register() {
  local jar="$1" email="$2" output="$3"
  curl --fail --silent --show-error -c "$jar" -b "$jar" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$email\",\"password\":\"correct-horse-battery\"}" \
    "$api/api/auth/register" >"$output"
}

csrf_token() {
  awk '$6 == "appstruct_csrf" { print $7 }' "$1"
}

create_organization() {
  local jar="$1" name="$2" output="$3" csrf
  csrf="$(csrf_token "$jar")"
  curl --fail --silent --show-error -c "$jar" -b "$jar" \
    -H "Content-Type: application/json" \
    -H "X-CSRF-Token: $csrf" -d "{\"name\":\"$name\"}" \
    "$api/api/tenant/organizations" >"$output"
}

assert_status() {
  local expected="$1" jar="$2" tenant="$3" path="$4" output="$5"
  local status
  status="$(curl --silent --show-error -o "$output" -w '%{http_code}' -b "$jar" \
    ${tenant:+-H "X-AppStruct-Tenant: $tenant"} "$api$path")"
  if [[ "$status" != "$expected" ]]; then
    echo "expected HTTP $expected from $path, got $status" >&2
    cat "$output" >&2
    exit 1
  fi
}

register "$first_jar" "first@example.com" "$temporary_root/first.json"
first_user="$(jq -er '.user.id' "$temporary_root/first.json")"
create_organization "$first_jar" "Alpha" "$temporary_root/alpha.json"
create_organization "$first_jar" "Beta" "$temporary_root/beta.json"
alpha="$(jq -er '.id' "$temporary_root/alpha.json")"
beta="$(jq -er '.id' "$temporary_root/beta.json")"

csrf="$(csrf_token "$first_jar")"
curl --fail --silent --show-error -b "$first_jar" \
  -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -H "X-AppStruct-Tenant: $alpha" \
  -d "{\"name\":\"Alpha project\",\"owner_id\":\"$first_user\"}" \
  "$api/api/projects/" >"$temporary_root/project.json"
project_id="$(jq -er '.id' "$temporary_root/project.json")"
jq -e --arg tenant "$alpha" '.tenant_id == $tenant' "$temporary_root/project.json" >/dev/null

assert_status 400 "$first_jar" "" "/api/projects/" "$temporary_root/missing.json"
jq -e '.error.code == "INVALID_TENANT"' "$temporary_root/missing.json" >/dev/null
assert_status 200 "$first_jar" "$alpha" "/api/projects/" "$temporary_root/alpha-list.json"
jq -e '.meta.total == 1' "$temporary_root/alpha-list.json" >/dev/null
assert_status 200 "$first_jar" "$beta" "/api/projects/" "$temporary_root/beta-list.json"
jq -e '.meta.total == 0' "$temporary_root/beta-list.json" >/dev/null
assert_status 404 "$first_jar" "$beta" "/api/projects/$project_id" "$temporary_root/cross.json"

register "$second_jar" "second@example.com" "$temporary_root/second.json"
create_organization "$second_jar" "Gamma" "$temporary_root/gamma.json"
gamma="$(jq -er '.id' "$temporary_root/gamma.json")"
assert_status 403 "$second_jar" "$alpha" "/api/projects/" "$temporary_root/non-member.json"
assert_status 403 "$first_jar" "$gamma" "/api/projects/" "$temporary_root/reverse.json"

pnpm --dir "$project/generated/web" exec tsc --noEmit
PLAYWRIGHT_API_URL="$api" PLAYWRIGHT_BASE_URL="http://127.0.0.1:$web_port" \
  pnpm exec playwright test --config playwright.tenant.config.ts
echo "Tenant PostgreSQL isolation E2E passed"
