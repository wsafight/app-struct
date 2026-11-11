#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init query 34880 56880
m6_require_test_db
m6_prepare_fixture m6-query-project
m6_start_dev APPSTRUCT_CURSOR_KEY=appstruct-query-e2e-fixed-signing-key
m6_wait_for_dev 240
python3 "$workspace/tests/e2e-query/queries.py" "http://127.0.0.1:$api_port"
env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$workspace/target/debug/appstruct" --project "$project" migrate status
m6_typecheck_web
