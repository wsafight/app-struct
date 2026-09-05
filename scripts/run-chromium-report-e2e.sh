#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init chromium-report 34000 56000 renderer-e2e
renderer_pid="" proxy_pid=""
socket_root="$(mktemp -d /tmp/as-render.XXXXXX)"
m6_cleanup_extra() {
  m6_stop_process "$proxy_pid" TERM || true
  m6_stop_process "$renderer_pid" TERM || true
  rm -r "$socket_root"
}
database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c 'SELECT current_database()')"
case "$database_name" in *test*|*e2e*) ;; *) echo "Dedicated test database required" >&2; exit 2 ;; esac
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public' >/dev/null
m6_prepare_fixture m7-report-project
cp "$workspace/tests/e2e/report-chromium.external.yaml" "$project/appstruct.yaml"
mkdir -p "$project/app"
cp -R "$workspace/examples/operations-demo/app/backend" "$project/app/"
renderer="$workspace/crates/appstruct-codegen/templates/report-renderer"
pnpm --dir "$renderer" install --frozen-lockfile --ignore-scripts
APPSTRUCT_RENDERER_SOCKET="$socket_root/real.sock" node "$renderer/server.mjs" >"$temporary_root/renderer.log" 2>&1 &
renderer_pid=$!
node "$workspace/tests/report-renderer-proxy.mjs" "$socket_root/proxy.sock" "$socket_root/real.sock" "$socket_root/open" >"$temporary_root/proxy.log" 2>&1 &
proxy_pid=$!
m6_start_dev APPSTRUCT_REPORT_RENDERER_SOCKET="$socket_root/proxy.sock" \
  APPSTRUCT_REPORT_SNAPSHOT_KEY=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA= APPSTRUCT_JOB_CONCURRENCY=1
m6_wait_for_dev 300
api="http://127.0.0.1:$api_port"
jar="$temporary_root/session.cookies"
curl -fsS -c "$jar" -H 'Content-Type: application/json' -d '{"email":"renderer@example.test","password":"AppStruct-Renderer-2026"}' "$api/api/auth/register" >"$temporary_root/user.json"
csrf="$(awk '$6 == "appstruct_csrf" {print $7}' "$jar")"
curl -fsS -b "$jar" -H 'Content-Type: application/json' -H "X-CSRF-Token: $csrf" -d '{"name":"Renderer acceptance"}' "$api/api/tenant/organizations" >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"
request=(-b "$jar" -H 'Content-Type: application/json' -H "X-CSRF-Token: $csrf" -H "X-AppStruct-Tenant: $tenant")
create_run() {
  curl -fsS "${request[@]}" -H "Idempotency-Key: $1" -d '{"data":{"title":"Transaction acceptance"}}' "$api/api/reports/templates/acceptance/runs" >"$temporary_root/run.json"
  run="$(jq -er '.id' "$temporary_root/run.json")"
  job="$(jq -er '.execution_job_id' "$temporary_root/run.json")"
}
wait_stage() {
  for ((attempt = 0; attempt < 100; attempt++)); do
    curl -fsS "${request[@]}" "$api/api/reports/runs/$run" >"$temporary_root/state.json"
    if [[ "$(jq -r '.stage' "$temporary_root/state.json")" == "$1" ]]; then return; fi
    sleep 0.1
  done
  cat "$temporary_root/state.json" >&2
  cat "$temporary_root/renderer.log" >&2
  tail -n 80 "$log" >&2
  return 1
}
assert_no_file() {
  [[ "$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c "SELECT count(*) FROM _appstruct_files WHERE object_key = 'reports/$tenant/$run.pdf'")" == 0 ]]
}
create_run renewal
wait_stage rendering
sleep 2.2
[[ "$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c "SELECT attempts = 1 AND locked_until > clock_timestamp() FROM _appstruct_jobs WHERE id = '$job'")" == t ]]
touch "$socket_root/open"
wait_stage succeeded
[[ "$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c "SELECT count(*) FROM _appstruct_report_runs r JOIN _appstruct_files f ON f.id = r.result_file_id WHERE r.id = '$run' AND f.tenant_id = r.tenant_id")" == 1 ]]
curl -fsS "${request[@]}" "$api/api/reports/runs/$run/download" >"$temporary_root/report.pdf"
[[ "$(head -c 5 "$temporary_root/report.pdf")" == '%PDF-' ]]

rm "$socket_root/open"
create_run cancellation
wait_stage rendering
curl -fsS "${request[@]}" -X POST "$api/api/reports/runs/$run/cancel" >"$temporary_root/cancel.json"
jq -e '.stage == "cancelled"' "$temporary_root/cancel.json" >/dev/null
touch "$socket_root/open"
sleep 0.5
assert_no_file

rm "$socket_root/open"
create_run lease-loss
wait_stage rendering
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c "UPDATE _appstruct_jobs SET locked_by = 'replacement-worker', attempts = attempts + 1, locked_until = clock_timestamp() + INTERVAL '60 seconds' WHERE id = '$job'" >/dev/null
sleep 0.5
touch "$socket_root/open"
sleep 1
assert_no_file
[[ "$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c "SELECT stage FROM _appstruct_report_runs WHERE id = '$run'")" == rendering ]]
curl -fsS "$api/metrics" >"$temporary_root/metrics.prom"
for outcome in succeeded cancelled lease_lost; do
  rg -Fq "appstruct_job_duration_seconds_count{kind=\"report\",outcome=\"$outcome\"} 1" "$temporary_root/metrics.prom" || {
    cat "$temporary_root/metrics.prom" >&2
    exit 1
  }
done
rg -q '^appstruct_jobs_in_flight 0$' "$temporary_root/metrics.prom"
echo "Chromium report protocol, lease renewal, running cancellation and stale worker publication checks passed"
