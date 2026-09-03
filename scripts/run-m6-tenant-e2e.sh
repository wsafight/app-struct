#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init tenant 33300 55300
m6_prepare_fixture m6-tenant-project
m6_start_dev
m6_wait_for_dev

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

if psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 >/dev/null 2>&1 <<SQL
INSERT INTO tasks (id, title, project_id, tenant_id)
VALUES ('00000000-0000-0000-0000-000000000001', 'Cross tenant', '$project_id', '$beta');
SQL
then
  echo "cross-tenant relation insert unexpectedly succeeded" >&2
  exit 1
fi
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 >/dev/null <<SQL
INSERT INTO tasks (id, title, project_id, tenant_id)
VALUES ('00000000-0000-0000-0000-000000000002', 'Same tenant', '$project_id', '$alpha');
SQL

m6_typecheck_web
m6_run_playwright playwright.tenant.config.ts "PLAYWRIGHT_API_URL=$api"
echo "Tenant PostgreSQL isolation E2E passed"
