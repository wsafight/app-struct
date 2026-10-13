#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init m5 33200 55200 m5-browser
startup_timeout="${APPSTRUCT_E2E_STARTUP_TIMEOUT:-180}"
m6_build_cli
pnpm install --frozen-lockfile
target/debug/appstruct --project "$temporary_root" new m5-browser --template dashboard
cp tests/e2e/dashboard.external.yaml "$project/appstruct.yaml"
mkdir -p "$project/.appstruct/cache" "$m6_backend_target"
ln -s "$m6_backend_target" "$project/.appstruct/cache/backend-target"

set -m
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  target/debug/appstruct --project "$project" dev --no-open --api-port "$api_port" --web-port "$web_port" \
  >"$log" 2>&1 &
dev_pid=$!
set +m

for ((attempt = 0; attempt < startup_timeout; attempt += 1)); do
  if curl --fail --silent --max-time 2 "http://127.0.0.1:$api_port/health/ready" >/dev/null 2>&1 \
    && curl --fail --silent --max-time 2 "http://127.0.0.1:$web_port/" >/dev/null 2>&1; then
    PLAYWRIGHT_API_URL="http://127.0.0.1:$api_port" \
      PLAYWRIGHT_BASE_URL="http://127.0.0.1:$web_port" \
      pnpm test:e2e

    missing_email="missing-$(date +%s)-$RANDOM@example.test"
    for attempt in {1..10}; do
      status="$(curl --silent --show-error -o "$temporary_root/reset-$attempt.json" \
        -w '%{http_code}' -H "Content-Type: application/json" \
        -d "{\"email\":\"$missing_email\"}" \
        "http://127.0.0.1:$api_port/api/auth/password/request")"
      if [[ "$status" != "204" ]]; then
        echo "expected password reset attempt $attempt to return HTTP 204, got $status" >&2
        cat "$temporary_root/reset-$attempt.json" >&2
        exit 1
      fi
    done
    status="$(curl --silent --show-error -o "$temporary_root/reset-limited.json" \
      -w '%{http_code}' -H "Content-Type: application/json" \
      -d "{\"email\":\"$missing_email\"}" \
      "http://127.0.0.1:$api_port/api/auth/password/request")"
    if [[ "$status" != "429" ]]; then
      echo "expected password reset attempt 11 to return HTTP 429, got $status" >&2
      cat "$temporary_root/reset-limited.json" >&2
      exit 1
    fi
    jq -e '.error.code == "RATE_LIMITED"' "$temporary_root/reset-limited.json" >/dev/null
    exit 0
  fi
  if ! kill -0 "$dev_pid" 2>/dev/null; then
    sed -n '1,240p' "$log" >&2
    exit 1
  fi
  sleep 1
done

sed -n '1,240p' "$log" >&2
echo "timed out waiting for AppStruct dev services" >&2
exit 1
