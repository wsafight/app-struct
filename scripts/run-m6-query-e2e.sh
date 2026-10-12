#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init query 34880 56880
database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
case "$database_name" in
  *e2e*|*test*) ;;
  *) echo "query tests require a dedicated database containing e2e or test in its name" >&2; exit 2 ;;
esac
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public' >/dev/null
m6_prepare_fixture m6-query-project
m6_start_dev APPSTRUCT_CURSOR_KEY=appstruct-query-e2e-fixed-signing-key
m6_wait_for_dev 240
python3 "$workspace/tests/e2e-query/queries.py" "http://127.0.0.1:$api_port"
env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$workspace/target/debug/appstruct" --project "$project" migrate status
m6_typecheck_web
