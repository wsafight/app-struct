#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init bulk 33800 55800
m6_prepare_fixture m6-bulk-project
m6_start_dev
m6_wait_for_dev

api="http://127.0.0.1:$api_port"
jar="$temporary_root/bulk.cookies"

curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" \
  -d '{"email":"bulk@example.com","password":"correct-horse-battery"}' \
  "$api/api/auth/register" >"$temporary_root/user.json"
user_id="$(jq -er '.user.id' "$temporary_root/user.json")"
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$jar")"

curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"Bulk Tenant"}' "$api/api/tenant/organizations" \
  >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"

csv_payload=$'code,title,secret\nalpha,Alpha,"private, ""quoted""\nline"\nalpha,Duplicate,duplicate-secret\nbeta,Beta,beta-secret\n'
curl --fail --silent --show-error -b "$jar" \
  -H "Content-Type: text/csv" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $tenant" --data-binary "$csv_payload" \
  "$api/api/entries/_import.csv" >"$temporary_root/import.json"
jq -e '
  .succeeded == ["0", "2"] and
  .failed == [{
    "id": "1",
    "code": "conflict",
    "message": "The record conflicts with existing data"
  }]
' "$temporary_root/import.json" >/dev/null || {
  cat "$temporary_root/import.json" >&2
  exit 1
}

curl --fail --silent --show-error -b "$jar" \
  -H "X-AppStruct-Tenant: $tenant" "$api/api/entries/?page_size=10" \
  >"$temporary_root/member-list.json"
jq -e '
  .meta.total == 2 and (.data | length == 2) and
  all(.data[]; has("secret") | not)
' "$temporary_root/member-list.json" >/dev/null || {
  cat "$temporary_root/member-list.json" >&2
  exit 1
}
alpha_id="$(jq -er '.data[] | select(.code == "alpha") | .id' "$temporary_root/member-list.json")"
alpha_revision="$(jq -er '.data[] | select(.code == "alpha") | .revision' "$temporary_root/member-list.json")"
beta_id="$(jq -er '.data[] | select(.code == "beta") | .id' "$temporary_root/member-list.json")"
beta_revision="$(jq -er '.data[] | select(.code == "beta") | .revision' "$temporary_root/member-list.json")"

curl --fail --silent --show-error -b "$jar" \
  -H "X-AppStruct-Tenant: $tenant" "$api/api/entries/_export.csv" \
  >"$temporary_root/member.csv"
member_header="$(sed -n '1p' "$temporary_root/member.csv")"
if [[ ",$member_header," == *,secret,* ]] || grep -Fq 'private' "$temporary_root/member.csv"; then
  echo "member CSV export exposed a restricted field" >&2
  cat "$temporary_root/member.csv" >&2
  exit 1
fi

jq -cn --arg id "$alpha_id" --argjson revision "$alpha_revision" '{
  ids: [$id], patch: {title: "Member update"}, expected_revisions: {($id): $revision}
}' >"$temporary_root/member-update-request.json"
curl --fail --silent --show-error -b "$jar" -X PATCH \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $tenant" \
  --data-binary "@$temporary_root/member-update-request.json" \
  "$api/api/entries/_bulk" >"$temporary_root/member-update.json"
jq -e --arg id "$alpha_id" '
  .succeeded == [] and
  .failed == [{"id": $id, "code": "forbidden", "message": "The operation is not allowed"}]
' "$temporary_root/member-update.json" >/dev/null || {
  cat "$temporary_root/member-update.json" >&2
  exit 1
}

psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 \
  -c "UPDATE \"_appstruct_auth_accounts\" SET roles = '[\"admin\"]'::jsonb WHERE user_id = '$user_id'::uuid" \
  >/dev/null

invalid_id="not-a-uuid"
jq -cn \
  --arg valid "$alpha_id" --arg invalid "$invalid_id" \
  --argjson revision "$alpha_revision" '{
    ids: [$valid, $invalid],
    patch: {title: "Mixed update"},
    expected_revisions: {($valid): $revision, ($invalid): 1}
  }' >"$temporary_root/mixed-update-request.json"
curl --fail --silent --show-error -b "$jar" -X PATCH \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $tenant" \
  --data-binary "@$temporary_root/mixed-update-request.json" \
  "$api/api/entries/_bulk" >"$temporary_root/mixed-update.json"
jq -e --arg valid "$alpha_id" --arg invalid "$invalid_id" '
  .succeeded == [$valid] and
  .failed == [{
    "id": $invalid,
    "code": "invalid_id",
    "message": "The resource identifier is invalid"
  }]
' "$temporary_root/mixed-update.json" >/dev/null || {
  cat "$temporary_root/mixed-update.json" >&2
  tail -n 120 "$log" >&2
  exit 1
}
alpha_revision=$((alpha_revision + 1))

jq -cn \
  --arg alpha "$alpha_id" --arg beta "$beta_id" \
  --argjson alpha_revision "$alpha_revision" --argjson beta_revision "$beta_revision" '{
    ids: [$alpha, $beta],
    patch: {title: "Revision-isolated update"},
    expected_revisions: {($alpha): $alpha_revision, ($beta): ($beta_revision + 100)}
  }' >"$temporary_root/revision-update-request.json"
curl --fail --silent --show-error -b "$jar" -X PATCH \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $tenant" \
  --data-binary "@$temporary_root/revision-update-request.json" \
  "$api/api/entries/_bulk" >"$temporary_root/revision-update.json"
jq -e --arg alpha "$alpha_id" --arg beta "$beta_id" '
  .succeeded == [$alpha] and
  .failed == [{
    "id": $beta,
    "code": "concurrent_modification",
    "message": "The record changed after it was loaded"
  }]
' "$temporary_root/revision-update.json" >/dev/null || {
  cat "$temporary_root/revision-update.json" >&2
  exit 1
}

jq -cn \
  --arg valid "$beta_id" --arg invalid "$invalid_id" \
  --argjson revision "$beta_revision" '{
    ids: [$invalid, $valid],
    expected_revisions: {($invalid): 1, ($valid): $revision}
  }' >"$temporary_root/delete-request.json"
curl --fail --silent --show-error -b "$jar" -X DELETE \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $tenant" \
  --data-binary "@$temporary_root/delete-request.json" \
  "$api/api/entries/_bulk" >"$temporary_root/delete.json"
jq -e --arg valid "$beta_id" --arg invalid "$invalid_id" '
  .succeeded == [$valid] and
  .failed == [{
    "id": $invalid,
    "code": "invalid_id",
    "message": "The resource identifier is invalid"
  }]
' "$temporary_root/delete.json" >/dev/null || {
  cat "$temporary_root/delete.json" >&2
  exit 1
}

curl --fail --silent --show-error -b "$jar" \
  -H "X-AppStruct-Tenant: $tenant" "$api/api/entries/_trash" \
  >"$temporary_root/trash.json"
jq -e --arg id "$beta_id" '.meta.total == 1 and .data[0].id == $id' \
  "$temporary_root/trash.json" >/dev/null
deleted_revision="$(jq -er '.data[0].revision' "$temporary_root/trash.json")"

jq -cn \
  --arg valid "$beta_id" --arg invalid "$invalid_id" \
  --argjson revision "$deleted_revision" '{
    ids: [$valid, $invalid],
    expected_revisions: {($valid): $revision, ($invalid): 1}
  }' >"$temporary_root/restore-request.json"
curl --fail --silent --show-error -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -H "X-AppStruct-Tenant: $tenant" \
  --data-binary "@$temporary_root/restore-request.json" \
  "$api/api/entries/_restore" >"$temporary_root/restore.json"
jq -e --arg valid "$beta_id" --arg invalid "$invalid_id" '
  .succeeded == [$valid] and
  .failed == [{
    "id": $invalid,
    "code": "invalid_id",
    "message": "The resource identifier is invalid"
  }]
' "$temporary_root/restore.json" >/dev/null || {
  cat "$temporary_root/restore.json" >&2
  exit 1
}

curl --fail --silent --show-error -b "$jar" \
  -H "X-AppStruct-Tenant: $tenant" "$api/api/entries/_export.csv" \
  >"$temporary_root/admin.csv"
admin_header="$(sed -n '1p' "$temporary_root/admin.csv")"
if [[ ",$admin_header," != *,secret,* ]]; then
  echo "admin CSV export omitted the restricted field" >&2
  cat "$temporary_root/admin.csv" >&2
  exit 1
fi
node -e '
  const csv = require("node:fs").readFileSync(process.argv[1], "utf8");
  if (!csv.includes("\"private, \"\"quoted\"\"\nline\"")) process.exit(1);
' "$temporary_root/admin.csv"

curl --fail --silent --show-error -b "$jar" \
  -H "X-AppStruct-Tenant: $tenant" "$api/api/audit/events?page_size=20" \
  >"$temporary_root/audit.json"
jq -e '
  .meta.total == 6 and
  ([.data[].operation] | sort) == ["create", "create", "delete", "restore", "update", "update"]
' "$temporary_root/audit.json" >/dev/null || {
  cat "$temporary_root/audit.json" >&2
  exit 1
}

m6_typecheck_web
echo "Bulk and CSV PostgreSQL E2E passed"
