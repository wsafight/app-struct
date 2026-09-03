#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init jobs 33600 55600
secondary_api_port="${APPSTRUCT_E2E_SECONDARY_API_PORT:-33601}"
webhook_port="${APPSTRUCT_E2E_WEBHOOK_PORT:-57600}"
secondary_pid=""
receiver_pid=""
sse_pid=""

m6_cleanup_pre() {
  if [[ -n "$sse_pid" ]] && kill -0 "$sse_pid" 2>/dev/null; then
    kill "$sse_pid" 2>/dev/null || true
    wait "$sse_pid" 2>/dev/null || true
  fi
}

m6_cleanup_extra() {
  if [[ -n "$secondary_pid" ]] && kill -0 "$secondary_pid" 2>/dev/null; then
    kill -INT "$secondary_pid" 2>/dev/null || true
    wait "$secondary_pid" 2>/dev/null || true
  fi
  if [[ -n "$receiver_pid" ]] && kill -0 "$receiver_pid" 2>/dev/null; then
    kill -INT "$receiver_pid" 2>/dev/null || true
    wait "$receiver_pid" 2>/dev/null || true
  fi
}

m6_prepare_fixture m6-jobs-project
perl -pi -e "s/__WEBHOOK_PORT__/$webhook_port/g" "$project/appstruct.yaml"

node "$project/webhook-receiver.mjs" "$webhook_port" \
  "$temporary_root/webhooks.jsonl" "operations-e2e-secret" \
  >"$temporary_root/webhook-receiver.log" 2>&1 &
receiver_pid=$!
for ((attempt = 0; attempt < 50; attempt += 1)); do
  if grep -q "ready" "$temporary_root/webhook-receiver.log"; then
    break
  fi
  if ! kill -0 "$receiver_pid" 2>/dev/null; then
    cat "$temporary_root/webhook-receiver.log" >&2
    exit 1
  fi
  sleep 0.1
done
grep -q "ready" "$temporary_root/webhook-receiver.log"

m6_start_dev \
  APPSTRUCT_WEBHOOK_OPERATIONS_SECRET=operations-e2e-secret \
  APPSTRUCT_WEBHOOK_HANGING_SECRET=hanging-e2e-secret
m6_wait_for_dev

api="http://127.0.0.1:$api_port"
secondary_api="http://127.0.0.1:$secondary_api_port"
backend="$project/.appstruct/cache/backend-target/debug/appstruct-generated-backend"
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  APPSTRUCT_BIND="127.0.0.1:$secondary_api_port" \
  APPSTRUCT_WEBHOOK_OPERATIONS_SECRET="operations-e2e-secret" \
  APPSTRUCT_WEBHOOK_HANGING_SECRET="hanging-e2e-secret" \
  "$backend" >"$temporary_root/secondary-api.log" 2>&1 &
secondary_pid=$!
for ((attempt = 0; attempt < 100; attempt += 1)); do
  if curl --fail --silent --max-time 2 "$secondary_api/health/ready" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$secondary_pid" 2>/dev/null; then
    tail -n 200 "$temporary_root/secondary-api.log" >&2
    exit 1
  fi
  sleep 0.1
done
curl --fail --silent --max-time 2 "$secondary_api/health/ready" >/dev/null

jar="$temporary_root/jobs.cookies"
email="jobs-$RANDOM-$$@example.com"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$email\",\"password\":\"correct-horse-battery\"}" \
  "$api/api/auth/register" >"$temporary_root/user.json"
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$jar")"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"Jobs Tenant"}' "$api/api/tenant/organizations" \
  >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"
user="$(jq -er '.user.id' "$temporary_root/user.json")"

DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" TENANT_ID="$tenant" \
  cargo run --quiet --manifest-path "$project/app/jobs-e2e/Cargo.toml"

jq -s -e '
  length == 1 and
  .[0].event == "project.created" and
  .[0].body.project_id == "project-1" and
  .[0].signatureValid == true and
  (.[0].delivery | test("^[0-9a-f-]{36}$")) and
  (.[0].timestamp | test("^[0-9]+$"))
' "$temporary_root/webhooks.jsonl" >/dev/null

schedule_jobs_before="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT COUNT(*) FROM \"_appstruct_jobs\" WHERE kind = 'maintenance.cleanup'")"
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c \
  "UPDATE \"_appstruct_job_schedules\" SET next_run_at = CURRENT_TIMESTAMP - INTERVAL '1 hour' WHERE name = 'cleanup'" \
  >/dev/null
for ((attempt = 0; attempt < 100; attempt += 1)); do
  schedule_jobs_after="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
    "SELECT COUNT(*) FROM \"_appstruct_jobs\" WHERE kind = 'maintenance.cleanup'")"
  if [[ "$schedule_jobs_after" == "$((schedule_jobs_before + 1))" ]]; then
    break
  fi
  sleep 0.05
done
sleep 0.2
schedule_state="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -F '|' -v ON_ERROR_STOP=1 -c \
  "SELECT COUNT(*), BOOL_AND(run_at < CURRENT_TIMESTAMP), (SELECT next_run_at > CURRENT_TIMESTAMP FROM \"_appstruct_job_schedules\" WHERE name = 'cleanup') FROM \"_appstruct_jobs\" WHERE kind = 'maintenance.cleanup'")"
[[ "$schedule_state" == "$((schedule_jobs_before + 1))|t|t" ]]

assert_realtime_status() {
  local expected="$1" path="$2" output="$3" status
  status="$(curl --silent --show-error -o "$output" -w '%{http_code}' -b "$jar" "$secondary_api$path")"
  if [[ "$status" != "$expected" ]]; then
    echo "expected realtime HTTP $expected, got $status from $path" >&2
    cat "$output" >&2
    exit 1
  fi
}

assert_realtime_status 400 "/api/realtime/events?tenant_id=$tenant" "$temporary_root/realtime-missing.json"
assert_realtime_status 400 "/api/realtime/events?tenant_id=$tenant&resource=unknown" "$temporary_root/realtime-unknown.json"
unauthorized_status="$(curl --silent --show-error -o "$temporary_root/realtime-unauthorized.json" \
  -w '%{http_code}' "$secondary_api/api/realtime/events?tenant_id=$tenant&resource=projects")"
[[ "$unauthorized_status" == "401" ]]

(curl --silent --show-error --no-buffer --max-time 12 -b "$jar" \
  "$secondary_api/api/realtime/events?tenant_id=$tenant&resource=projects" \
  >"$temporary_root/realtime.events" || true) &
sse_pid=$!
for ((attempt = 0; attempt < 100; attempt += 1)); do
  presence_count="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
    "SELECT COUNT(*) FROM \"_appstruct_realtime_presence\" WHERE tenant_id = '$tenant' AND resource = 'projects' AND expires_at > CURRENT_TIMESTAMP")"
  [[ "$presence_count" == "1" ]] && break
  sleep 0.05
done
[[ "$presence_count" == "1" ]]
presence_expiry="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT EXTRACT(EPOCH FROM expires_at)::bigint FROM \"_appstruct_realtime_presence\" WHERE tenant_id = '$tenant' AND resource = 'projects'")"
curl --fail --silent --show-error -b "$jar" \
  "$secondary_api/api/realtime/presence?tenant_id=$tenant&resource=projects" \
  >"$temporary_root/presence.json"
jq -e --arg actor "$user" --arg tenant "$tenant" '
  .data | length == 1 and .[0].actor_id == $actor and .[0].tenant_id == $tenant and
  .[0].resource == "projects" and .[0].record_id == null
' "$temporary_root/presence.json" >/dev/null

curl --fail --silent --show-error -b "$jar" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -H "X-AppStruct-Tenant: $tenant" \
  -d '{"email":"realtime-other@example.test"}' \
  "$api/api/users/" >"$temporary_root/realtime-user.json"
curl --fail --silent --show-error -b "$jar" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -H "X-AppStruct-Tenant: $tenant" \
  -d "{\"name\":\"Realtime project\",\"owner_id\":\"$user\"}" \
  "$api/api/projects/" >"$temporary_root/realtime-project.json"
project_id="$(jq -er '.id' "$temporary_root/realtime-project.json")"
for ((attempt = 0; attempt < 100; attempt += 1)); do
  grep -Eq '^event: ?project.created' "$temporary_root/realtime.events" && break
  sleep 0.05
done
grep -Eq '^event: ?project.created' "$temporary_root/realtime.events"
if grep -Eq '^event: ?user.created' "$temporary_root/realtime.events"; then
  echo "projects realtime subscription received a users event" >&2
  exit 1
fi
sed -n 's/^data: *//p' "$temporary_root/realtime.events" >"$temporary_root/realtime-data.jsonl"
jq -s -e --arg record "$project_id" '
  [.[] | select(.event == "project.created")] | length == 1 and
  .[0].resource == "projects" and .[0].record_id == $record and
  .[0].data == {resource: "projects", record_id: $record}
' "$temporary_root/realtime-data.jsonl" >/dev/null
fanout_rows="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT COUNT(*) FROM \"_appstruct_realtime_events\" WHERE event = 'project.created' AND record_id = '$project_id'")"
[[ "$fanout_rows" == "1" ]]

lock_query="tenant_id=$tenant&resource=projects&record_id=$project_id"
curl --fail --silent --show-error -b "$jar" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -X POST -d '{"ttl_seconds":5}' \
  "$api/api/realtime/locks?$lock_query" >"$temporary_root/lock.json"
lock_token="$(jq -er '.lease_token' "$temporary_root/lock.json")"
curl --fail --silent --show-error -b "$jar" \
  "$secondary_api/api/realtime/locks?$lock_query" >"$temporary_root/lock-status.json"
jq -e --arg token "$lock_token" --arg record "$project_id" \
  '.data.lease_token == $token and .data.record_id == $record' \
  "$temporary_root/lock-status.json" >/dev/null
lock_conflict="$(curl --silent --show-error -o "$temporary_root/lock-conflict.json" \
  -w '%{http_code}' -b "$jar" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -X POST -d '{"ttl_seconds":5}' \
  "$secondary_api/api/realtime/locks?$lock_query")"
[[ "$lock_conflict" == "409" ]]

sleep 6
renewed_expiry="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT EXTRACT(EPOCH FROM expires_at)::bigint FROM \"_appstruct_realtime_presence\" WHERE tenant_id = '$tenant' AND resource = 'projects'")"
(( renewed_expiry > presence_expiry ))
curl --fail --silent --show-error -b "$jar" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -X POST -d '{"ttl_seconds":5}' \
  "$secondary_api/api/realtime/locks?$lock_query" >"$temporary_root/lock-reacquired.json"
replacement_token="$(jq -er '.lease_token' "$temporary_root/lock-reacquired.json")"
[[ "$replacement_token" != "$lock_token" ]]
curl --fail --silent --show-error -b "$jar" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $csrf" -X PATCH -d '{"ttl_seconds":30}' \
  "$api/api/realtime/locks/$replacement_token?$lock_query" >"$temporary_root/lock-renewed.json"
jq -e --arg token "$replacement_token" '.lease_token == $token' \
  "$temporary_root/lock-renewed.json" >/dev/null
curl --fail --silent --show-error -b "$jar" -H "X-CSRF-Token: $csrf" \
  -X DELETE "$secondary_api/api/realtime/locks/$replacement_token?$lock_query" >/dev/null
curl --fail --silent --show-error -b "$jar" \
  "$api/api/realtime/locks?$lock_query" >"$temporary_root/lock-released.json"
jq -e '.data == null' "$temporary_root/lock-released.json" >/dev/null
wait "$sse_pid" || true
sse_pid=""
for ((attempt = 0; attempt < 100; attempt += 1)); do
  presence_count="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
    "SELECT COUNT(*) FROM \"_appstruct_realtime_presence\" WHERE tenant_id = '$tenant' AND resource = 'projects'")"
  [[ "$presence_count" == "0" ]] && break
  sleep 0.05
done
[[ "$presence_count" == "0" ]]

curl --fail --silent --show-error -c "$jar" -b "$jar" \
  "$api/api/admin/jobs" >"$temporary_root/jobs.json"
dead_job="$(jq -er '.data[] | select(.kind == "fail" and .status == "dead") | .id' "$temporary_root/jobs.json")"
succeeded_job="$(jq -er '.data[] | select(.kind == "succeed" and .status == "succeeded") | .id' "$temporary_root/jobs.json")"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "X-CSRF-Token: $csrf" -X POST \
  "$api/api/admin/jobs/$dead_job/retry" >"$temporary_root/retried.json"
jq -e '.id == $id and .status == "queued" and .attempts == 0' \
  --arg id "$dead_job" "$temporary_root/retried.json" >/dev/null
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "X-CSRF-Token: $csrf" -X POST \
  "$api/api/admin/jobs/$succeeded_job/replay" >"$temporary_root/replayed.json"
jq -e '.id != $id and .status == "queued" and .attempts == 0' \
  --arg id "$succeeded_job" "$temporary_root/replayed.json" >/dev/null
curl --fail --silent --show-error -b "$jar" \
  "$api/api/admin/webhooks" >"$temporary_root/admin-webhooks.json"
dead_delivery="$(jq -er '.data[] | select(.endpoint == "hanging" and .status == "dead") | .id' "$temporary_root/admin-webhooks.json")"
succeeded_delivery="$(jq -er '.data[] | select(.endpoint == "operations" and .status == "succeeded") | .id' "$temporary_root/admin-webhooks.json")"
curl --fail --silent --show-error -b "$jar" -H "X-CSRF-Token: $csrf" -X POST \
  "$api/api/admin/webhooks/$dead_delivery/retry" >"$temporary_root/retried-webhook.json"
jq -e '.id == $id and .status == "pending" and .attempts == 0 and .last_error == null' \
  --arg id "$dead_delivery" "$temporary_root/retried-webhook.json" >/dev/null
curl --fail --silent --show-error -b "$jar" -H "X-CSRF-Token: $csrf" -X POST \
  "$api/api/admin/webhooks/$succeeded_delivery/replay" >"$temporary_root/replayed-webhook.json"
jq -e '.id != $id and .status == "pending" and .attempts == 0' \
  --arg id "$succeeded_delivery" "$temporary_root/replayed-webhook.json" >/dev/null
m6_typecheck_web

perl -0pi -e 's/\n    schedules:\n      cleanup:\n        cron: "\*\/15 \* \* \* \*"\n        queue: default\n        kind: maintenance\.cleanup\n        payload: '\''\{"scope":"expired"\}'\''\n/\n/' "$project/appstruct.yaml"
for ((attempt = 0; attempt < 180; attempt += 1)); do
  schedule_enabled="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
    "SELECT enabled FROM \"_appstruct_job_schedules\" WHERE name = 'cleanup'")"
  [[ "$schedule_enabled" == "f" ]] && break
  if ! kill -0 "$dev_pid" 2>/dev/null; then
    tail -n 260 "$log" >&2
    exit 1
  fi
  sleep 1
done
[[ "$schedule_enabled" == "f" ]]
m6_wait_for_dev

kill -INT -- "-$dev_pid"
wait "$dev_pid"
dev_pid=""
kill -INT "$secondary_pid"
wait "$secondary_pid"
secondary_pid=""
grep -q "shutdown signal received" "$log"
grep -q "module stopped" "$log"
grep -q "shutdown signal received" "$temporary_root/secondary-api.log"

if APPSTRUCT_ENV=production DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  APPSTRUCT_BIND="127.0.0.1:0" "$backend" >"$temporary_root/config-error.log" 2>&1
then
  echo "invalid production mail configuration unexpectedly started" >&2
  exit 1
fi
grep -q "capture mail provider is forbidden in production" "$temporary_root/config-error.log"
if grep -q "panicked at" "$temporary_root/config-error.log"; then
  cat "$temporary_root/config-error.log" >&2
  exit 1
fi
echo "Jobs, schedules, webhooks, realtime, and admin PostgreSQL E2E passed"
