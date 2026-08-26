#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
  echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
  exit 2
fi

workspace="$(cd "$(dirname "$0")/.." && pwd)"
api_port="${APPSTRUCT_E2E_API_PORT:-33700}"
web_port="${APPSTRUCT_E2E_WEB_PORT:-55700}"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-m6-saas.XXXXXX")"
project="$temporary_root/saas-e2e"
log="$temporary_root/dev.log"
dev_pid=""

cleanup() {
  if [[ -n "$dev_pid" ]] && kill -0 "$dev_pid" 2>/dev/null; then
    kill -INT -- "-$dev_pid" 2>/dev/null || true
    wait "$dev_pid" 2>/dev/null || true
  fi
  case "$temporary_root" in
    "${TMPDIR:-/tmp}"/appstruct-m6-saas.*) rm -r "$temporary_root" ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cd "$workspace"
cargo build --locked -p appstruct-cli
pnpm install --frozen-lockfile
target/debug/appstruct --project "$temporary_root" new saas-e2e --template saas
cp tests/e2e/saas.external.yaml "$project/appstruct.yaml"

preset="$(target/debug/appstruct --project "$project" preset show)"
[[ "$preset" == *"appstruct/saas 1"* ]]
[[ "$preset" == *"modules: audit, auth, file, jobs, mail, rbac, tenant"* ]]

set -m
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  target/debug/appstruct --project "$project" dev --api-port "$api_port" --web-port "$web_port" \
  >"$log" 2>&1 &
dev_pid=$!
set +m

for ((attempt = 0; attempt < 240; attempt += 1)); do
  if curl --fail --silent --max-time 2 "http://127.0.0.1:$api_port/health/ready" >/dev/null 2>&1 \
    && curl --fail --silent --max-time 2 "http://127.0.0.1:$web_port/" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$dev_pid" 2>/dev/null; then
    sed -n '1,240p' "$log" >&2
    exit 1
  fi
  sleep 1
done
curl --fail --silent --max-time 2 "http://127.0.0.1:$api_port/health/ready" >/dev/null
curl --fail --silent --max-time 2 "http://127.0.0.1:$web_port/" >/dev/null

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

pnpm --dir "$project/generated/web" exec tsc --noEmit
SAAS_E2E_EMAIL="$email" SAAS_E2E_PASSWORD="$password" \
  PLAYWRIGHT_BASE_URL="http://127.0.0.1:$web_port" \
  pnpm exec playwright test --config playwright.saas.config.ts
echo "SaaS template PostgreSQL and browser E2E passed"
