#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init benchmark 34100 56100 benchmark
database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
case "$database_name" in *test*|*e2e*) ;; *) echo "Dedicated test database required" >&2; exit 2 ;; esac
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public' >/dev/null
m6_prepare_fixture m6-bulk-project
profile="${APPSTRUCT_BENCH_PROFILE:-release}"
case "$profile" in
  debug)
    m6_start_dev
    ;;
  release)
    cli="$workspace/target/debug/appstruct"
    env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" generate
    env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" migrate dev --accept
    env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" migrate apply
    if [[ -f "$project/generated/server/Cargo.toml" ]]; then
      manifest="$project/generated/server/Cargo.toml"
      backend="$m6_backend_target/release/appstruct-generated-server"
    else
      manifest="$project/generated/backend/Cargo.toml"
      backend="$m6_backend_target/release/appstruct-generated-backend"
    fi
    cargo build --release --locked --manifest-path "$manifest" --target-dir "$m6_backend_target"
    set -m
    env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" APPSTRUCT_BIND="127.0.0.1:$api_port" \
      "$backend" >"$log" 2>&1 &
    dev_pid=$!
    set +m
    ;;
  *)
    echo "APPSTRUCT_BENCH_PROFILE must be debug or release" >&2
    exit 2
    ;;
esac
m6_wait_for_dev 300
APPSTRUCT_BENCH_PROFILE="$profile" node "$workspace/tests/benchmarks/postgres-api.mjs" "http://127.0.0.1:$api_port" \
  "${APPSTRUCT_BENCH_OUTPUT:-$workspace/output/benchmarks/postgres-api.json}"
