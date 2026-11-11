#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init audit 33400 55400
m6_prepare_fixture m6-audit-project
m6_start_dev
m6_wait_for_dev

api="http://127.0.0.1:$api_port"
admin_jar="$temporary_root/admin.cookies"
other_jar="$temporary_root/other.cookies"

m6_register "$admin_jar" "audit-admin@example.com" "$temporary_root/admin.json"
admin_id="$(jq -er '.user.id' "$temporary_root/admin.json")"
m6_create_org "$admin_jar" "Alpha" "$temporary_root/alpha.json"
m6_create_org "$admin_jar" "Beta" "$temporary_root/beta.json"
alpha="$(jq -er '.id' "$temporary_root/alpha.json")"
beta="$(jq -er '.id' "$temporary_root/beta.json")"
csrf="$(m6_csrf_token "$admin_jar")"

curl --fail --silent --show-error -b "$admin_jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $alpha" \
  -d "{\"name\":\"Original project\",\"owner_id\":\"$admin_id\"}" \
  "$api/api/projects/" >"$temporary_root/project.json"
project_id="$(jq -er '.id' "$temporary_root/project.json")"
revision="$(jq -er '.revision' "$temporary_root/project.json")"

curl --fail --silent --show-error -b "$admin_jar" -X PATCH \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $alpha" -H "If-Match: \"rev-$revision\"" \
  -d '{"name":"Renamed project"}' "$api/api/projects/$project_id" \
  >"$temporary_root/updated.json"
revision="$(jq -er '.revision' "$temporary_root/updated.json")"

curl --fail --silent --show-error -b "$admin_jar" -X DELETE \
  -H "X-CSRF-Token: $csrf" -H "X-AppStruct-Tenant: $alpha" \
  -H "If-Match: \"rev-$revision\"" "$api/api/projects/$project_id" >/dev/null

m6_assert_status 200 "$admin_jar" "$alpha" GET "/api/audit/events" "$temporary_root/events.json"
jq -e --arg actor "$admin_id" --arg tenant "$alpha" --arg record "$project_id" '
  .meta.total == 3 and
  (.data | length == 3) and
  (all(.data[]; .actor_id == $actor and .tenant_id == $tenant and .record_id == $record)) and
  ([.data[].operation] | sort) == ["create", "delete", "update"] and
  ([.data[] | select(.operation == "create" and .before == null and .after.name == "Original project")] | length) == 1 and
  ([.data[] | select(.operation == "update" and .before.name == "Original project" and .after.name == "Renamed project")] | length) == 1 and
  ([.data[] | select(.operation == "delete" and .before.name == "Renamed project" and .after == null)] | length) == 1
' "$temporary_root/events.json" >/dev/null

m6_assert_status 400 "$admin_jar" "" GET "/api/audit/events" "$temporary_root/missing-tenant.json"
jq -e '.error.code == "INVALID_TENANT"' "$temporary_root/missing-tenant.json" >/dev/null
m6_assert_status 200 "$admin_jar" "$beta" GET "/api/audit/events" "$temporary_root/beta-events.json"
jq -e '.meta.total == 0 and .data == []' "$temporary_root/beta-events.json" >/dev/null
m6_assert_status 405 "$admin_jar" "$alpha" DELETE "/api/audit/events" "$temporary_root/immutable.json"

m6_register "$other_jar" "audit-member@example.com" "$temporary_root/other.json"
other_id="$(jq -er '.user.id' "$temporary_root/other.json")"
m6_create_org "$other_jar" "Gamma" "$temporary_root/gamma.json"
gamma="$(jq -er '.id' "$temporary_root/gamma.json")"
m6_assert_status 403 "$other_jar" "$alpha" GET "/api/audit/events" "$temporary_root/foreign.json"
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 \
  -c "UPDATE \"_appstruct_auth_accounts\" SET roles = '[\"member\"]'::jsonb WHERE user_id = '$other_id'::uuid" \
  >/dev/null
m6_assert_status 403 "$other_jar" "$gamma" GET "/api/audit/events" "$temporary_root/role-denied.json"

m6_typecheck_web
m6_run_playwright playwright.audit.config.ts "PLAYWRIGHT_API_URL=$api"
echo "Audit PostgreSQL and browser E2E passed"
