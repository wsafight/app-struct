#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init mail 33500 55500
m6_prepare_fixture m6-mail-project
m6_start_dev
m6_wait_for_dev

api="http://127.0.0.1:$api_port"
jar="$temporary_root/mail.cookies"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" \
  -d '{"email":"mail@example.com","password":"correct-horse-battery"}' \
  "$api/api/auth/register" >"$temporary_root/user.json"
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$jar")"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"Mail Tenant"}' "$api/api/tenant/organizations" \
  >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"

DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" TENANT_ID="$tenant" \
  cargo run --quiet --manifest-path "$project/app/mail-e2e/Cargo.toml"
APPSTRUCT_ENV=production DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  cargo run --quiet --manifest-path "$project/app/mail-e2e/Cargo.toml" -- production-check

psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -Atc \
  "SELECT provider || '|' || template || '|' || sender || '|' || recipient || '|' || subject || '|' || text_body || '|' || COALESCE(html_body, '') || '|' || tenant_id::text FROM \"_appstruct_mail_deliveries\"" \
  >"$temporary_root/capture.txt"
expected="capture|project-created|AppStruct <notifications@example.com>|ada@example.com|Project Mercury created|Hello Ada, your project is ready.|<p>Hello Ada, your project is ready.</p>|$tenant"
if [[ "$(cat "$temporary_root/capture.txt")" != "$expected" ]]; then
  echo "captured mail did not match rendered contract" >&2
  cat "$temporary_root/capture.txt" >&2
  exit 1
fi

curl --fail --silent --show-error -b "$jar" \
  "$api/api/admin/mail?search=Mercury&page=1&page_size=10" \
  >"$temporary_root/admin-mail.json"
mail_id="$(jq -er '.data[0].id' "$temporary_root/admin-mail.json")"
jq -e --arg tenant "$tenant" '
  .meta == {page: 1, page_size: 10, total: 1} and
  (.data | length) == 1 and .data[0].subject == "Project Mercury created" and
  .data[0].recipient == "ada@example.com" and .data[0].tenant_id == $tenant
' "$temporary_root/admin-mail.json" >/dev/null
curl --fail --silent --show-error -b "$jar" \
  "$api/api/admin/mail/$mail_id" >"$temporary_root/admin-mail-detail.json"
jq -e '
  .text_body == "Hello Ada, your project is ready." and
  .html_body == "<p>Hello Ada, your project is ready.</p>"
' "$temporary_root/admin-mail-detail.json" >/dev/null

m6_typecheck_web
echo "Mail PostgreSQL capture and admin E2E passed"
