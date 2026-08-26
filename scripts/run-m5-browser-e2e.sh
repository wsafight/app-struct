#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
  echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
  exit 2
fi

workspace="$(cd "$(dirname "$0")/.." && pwd)"
api_port="${APPSTRUCT_E2E_API_PORT:-33200}"
web_port="${APPSTRUCT_E2E_WEB_PORT:-55200}"
startup_timeout="${APPSTRUCT_E2E_STARTUP_TIMEOUT:-180}"
if ! [[ "$startup_timeout" =~ ^[1-9][0-9]*$ ]]; then
  echo "APPSTRUCT_E2E_STARTUP_TIMEOUT must be a positive integer" >&2
  exit 2
fi
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-m5-e2e.XXXXXX")"
project="$temporary_root/m5-browser"
log="$temporary_root/dev.log"
dev_pid=""

cleanup() {
  if [[ -n "$dev_pid" ]] && kill -0 "$dev_pid" 2>/dev/null; then
    kill -INT -- "-$dev_pid" 2>/dev/null || true
    wait "$dev_pid" 2>/dev/null || true
  fi
  case "$temporary_root" in
    "${TMPDIR:-/tmp}"/appstruct-m5-e2e.*) rm -r "$temporary_root" ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cd "$workspace"
cargo build --locked -p appstruct-cli
pnpm install --frozen-lockfile
target/debug/appstruct --project "$temporary_root" new m5-browser --template dashboard
cp tests/e2e/dashboard.external.yaml "$project/appstruct.yaml"

set -m
DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  target/debug/appstruct --project "$project" dev --api-port "$api_port" --web-port "$web_port" \
  >"$log" 2>&1 &
dev_pid=$!
set +m

for ((attempt = 0; attempt < startup_timeout; attempt += 1)); do
  if curl --fail --silent --max-time 2 "http://127.0.0.1:$api_port/health/ready" >/dev/null 2>&1 \
    && curl --fail --silent --max-time 2 "http://127.0.0.1:$web_port/" >/dev/null 2>&1; then
    PLAYWRIGHT_API_URL="http://127.0.0.1:$api_port" \
      PLAYWRIGHT_BASE_URL="http://127.0.0.1:$web_port" \
      pnpm test:e2e
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
