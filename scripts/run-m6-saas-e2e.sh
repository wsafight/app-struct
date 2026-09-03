#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init saas 33700 55700 saas-e2e
m6_build_cli
target/debug/appstruct --project "$temporary_root" new saas-e2e --template saas
cp tests/e2e/saas.external.yaml "$project/appstruct.yaml"

preset="$(target/debug/appstruct --project "$project" preset show)"
[[ "$preset" == *"appstruct/saas 1"* ]]
[[ "$preset" == *"modules: audit, auth, file, jobs, mail, rbac, tenant"* ]]

m6_start_dev
m6_wait_for_dev 240
m6_wait_for_url "http://127.0.0.1:$web_port/" "$dev_pid" "$log" 240 1

module_tables="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM pg_class WHERE relkind = 'r' AND relname IN ('_appstruct_audit_events', '_appstruct_files', '_appstruct_jobs', '_appstruct_mail_deliveries', '_appstruct_tenant_organizations')")"
[[ "$module_tables" == "5" ]]

api="http://127.0.0.1:$api_port"
email="saas-admin@example.test"
password="AppStruct-SaaS-E2E-2026"
cookie_jar="$temporary_root/admin.cookies"
curl --fail --silent --show-error -c "$cookie_jar" -b "$cookie_jar" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$email\",\"password\":\"$password\"}" \
  "$api/api/auth/register" >"$temporary_root/admin.json"
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  target/debug/appstruct --project "$project" auth bootstrap-admin --email "$email"
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  target/debug/appstruct --project "$project" auth bootstrap-admin --email "$email"
bootstrap_events="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM \"_appstruct_audit_events\" WHERE entity = '_appstruct_auth_accounts' AND operation = 'update'")"
[[ "$bootstrap_events" == "1" ]]
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$cookie_jar")"
curl --fail --silent --show-error -c "$cookie_jar" -b "$cookie_jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"Alpha workspace"}' "$api/api/tenant/organizations" \
  >"$temporary_root/organization.json"

m6_typecheck_web
m6_run_playwright playwright.saas.config.ts \
  "SAAS_E2E_EMAIL=$email" "SAAS_E2E_PASSWORD=$password"
echo "SaaS template PostgreSQL and browser E2E passed"
