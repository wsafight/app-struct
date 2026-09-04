#!/usr/bin/env bash

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  echo "scripts/m6-e2e-common.sh must be sourced" >&2
  exit 2
fi

m6_init() {
  local suite="$1" api_default="$2" web_default="$3" project_name="${4:-project}"
  if [[ -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
    echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
    exit 2
  fi
  workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  api_port="${APPSTRUCT_E2E_API_PORT:-$api_default}"
  web_port="${APPSTRUCT_E2E_WEB_PORT:-$web_default}"
  m6_suite="$suite"
  m6_tmp_base="${TMPDIR:-/tmp}"
  m6_tmp_base="${m6_tmp_base%/}"
  m6_backend_target="$workspace/target/m6-e2e-$suite"
  temporary_root="$(mktemp -d "$m6_tmp_base/appstruct-m6-$suite.XXXXXX")"
  project="$temporary_root/$project_name"
  log="$temporary_root/dev.log"
  dev_pid=""
  trap m6_cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
}

m6_stop_process() {
  local pid="$1" signal="${2:-INT}" target="${3:-$1}" attempts="${4:-100}"
  local attempt
  if [[ -z "$pid" ]]; then
    return 0
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    wait "$pid" 2>/dev/null || true
    return 0
  fi

  kill "-$signal" -- "$target" 2>/dev/null || true
  for ((attempt = 0; attempt < attempts; attempt += 1)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.1
  done

  echo "process $pid did not stop after SIG$signal; escalating to SIGTERM" >&2
  kill -TERM -- "$target" 2>/dev/null || true
  for ((attempt = 0; attempt < 50; attempt += 1)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null || true
      return 124
    fi
    sleep 0.1
  done
  kill -KILL -- "$target" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  return 124
}

m6_cleanup() {
  if declare -F m6_cleanup_pre >/dev/null; then
    m6_cleanup_pre
  fi
  if [[ -n "${dev_pid:-}" ]] && kill -0 "$dev_pid" 2>/dev/null; then
    m6_stop_process "$dev_pid" INT "-$dev_pid" || true
  fi
  if declare -F m6_cleanup_extra >/dev/null; then
    m6_cleanup_extra
  fi
  case "${temporary_root:-}" in
    "$m6_tmp_base"/appstruct-m6-"$m6_suite".*) rm -r "$temporary_root" ;;
    "") ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
}

m6_prepare_fixture() {
  local fixture="$1"
  mkdir -p "$project"
  cp -R "$workspace/tests/fixtures/$fixture/." "$project/"
  mkdir -p "$project/.appstruct/cache" "$m6_backend_target"
  ln -s "$m6_backend_target" "$project/.appstruct/cache/backend-target"
  cd "$workspace"
  cargo build --locked -p appstruct-cli
}

m6_build_cli() {
  cd "$workspace"
  cargo build --locked -p appstruct-cli
}

m6_start_dev() {
  set -m
  env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$@" \
    "$workspace/target/debug/appstruct" --project "$project" dev \
    --api-port "$api_port" --web-port "$web_port" >"$log" 2>&1 &
  dev_pid=$!
  set +m
}

m6_wait_for_url() {
  local url="$1" pid="$2" process_log="$3" attempts="${4:-180}" delay="${5:-1}"
  local attempt
  for ((attempt = 0; attempt < attempts; attempt += 1)); do
    if curl --fail --silent --max-time 2 "$url" >/dev/null 2>&1; then
      return 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      tail -n 240 "$process_log" >&2
      return 1
    fi
    sleep "$delay"
  done
  curl --fail --silent --max-time 2 "$url" >/dev/null
}

m6_wait_for_dev() {
  m6_wait_for_url "http://127.0.0.1:$api_port/health/ready" "$dev_pid" "$log" "${1:-180}" 1
}

m6_typecheck_web() {
  pnpm --dir "$project/generated/web" exec tsc6 --noEmit
}

m6_run_playwright() {
  local config="$1"
  shift
  pnpm install --frozen-lockfile
  env PLAYWRIGHT_BASE_URL="http://127.0.0.1:$web_port" "$@" \
    pnpm exec playwright test --config "$config"
}
