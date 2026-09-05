#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init benchmark 34100 56100 benchmark
database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
case "$database_name" in *test*|*e2e*) ;; *) echo "Dedicated test database required" >&2; exit 2 ;; esac
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public' >/dev/null
m6_prepare_fixture m6-bulk-project
m6_start_dev
m6_wait_for_dev 300
node "$workspace/tests/benchmarks/postgres-api.mjs" "http://127.0.0.1:$api_port" \
  "${APPSTRUCT_BENCH_OUTPUT:-$workspace/output/benchmarks/postgres-api.json}"
