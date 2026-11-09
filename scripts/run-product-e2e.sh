#!/usr/bin/env bash
set -euo pipefail

: "${APPSTRUCT_E2E_DATABASE_URL:?Set a dedicated PostgreSQL test database URL}"
workspace="$(cd "$(dirname "$0")/.." && pwd)"
cd "$workspace"
database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -XAt -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
case "$database_name" in
  *test*|*e2e*) ;;
  *) echo "refusing non-test database '$database_name'" >&2; exit 2 ;;
esac

temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-product-e2e.XXXXXX")"
trap 'rm -r "$temporary_root"' EXIT
export APPSTRUCT_REQUIRE_PRODUCT_TESTS=1
export APPSTRUCT_ENV=development
export APPSTRUCT_AUTH_MAIL_MODE=capture
export APPSTRUCT_COOKIE_SECURE=false
export APPSTRUCT_AUTH_MFA_KEY="$(openssl rand -base64 32)"
export APPSTRUCT_REPORT_SNAPSHOT_KEY="$(openssl rand -base64 32)"
export APPSTRUCT_TEAM_SEAT_LIMIT=2
export APPSTRUCT_MONTHLY_FREE_CREDITS=20
export APPSTRUCT_STRIPE_SECRET_KEY=sk_test_appstruct_local
export APPSTRUCT_STRIPE_WEBHOOK_SECRET=whsec_appstruct_local
export APPSTRUCT_STRIPE_PRICE_PRO=price_appstruct_local
export APPSTRUCT_FILE_ROOT="$temporary_root/files"
export APPSTRUCT_AI_ENABLED=true
export APPSTRUCT_AI_MODEL=appstruct-local-stub
export APPSTRUCT_AI_ENDPOINT="http://127.0.0.1:${APPSTRUCT_E2E_AI_PORT:-55891}/api/generate"

cargo test --locked -p appstruct-codegen --test product_journey --test auth_security_behavior -- --nocapture
