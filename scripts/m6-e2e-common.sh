#!/usr/bin/env bash

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  echo "scripts/m6-e2e-common.sh must be sourced" >&2
  exit 2
fi

m6_init() {
  local suite="$1" api_default="$2" web_default="$3" project_name="${4:-project}" database_required="${5:-true}"
  if [[ "$database_required" != "false" && -z "${APPSTRUCT_E2E_DATABASE_URL:-}" ]]; then
    echo "APPSTRUCT_E2E_DATABASE_URL must name a dedicated PostgreSQL test database" >&2
    exit 2
  fi
  if [[ -n "${APPSTRUCT_E2E_STARTUP_TIMEOUT:-}" ]] \
    && ! [[ "$APPSTRUCT_E2E_STARTUP_TIMEOUT" =~ ^[1-9][0-9]*$ ]]; then
    echo "APPSTRUCT_E2E_STARTUP_TIMEOUT must be a positive integer" >&2
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
  set -E
  trap 'printf "%s E2E failed at %s:%s\n" "$m6_suite" "${BASH_SOURCE[0]}" "$LINENO" >&2' ERR
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
  m6_preserve_logs
  case "${temporary_root:-}" in
    "$m6_tmp_base"/appstruct-m6-"$m6_suite".*) rm -r "$temporary_root" ;;
    "") ;;
    *) echo "refusing to remove unexpected path: $temporary_root" >&2 ;;
  esac
  if [[ "${APPSTRUCT_KEEP_TEST_TARGETS:-0}" != 1 ]]; then
    case "${m6_backend_target:-}" in
      "$workspace"/target/m6-e2e-"$m6_suite")
        [[ ! -d "$m6_backend_target" ]] || rm -r -- "$m6_backend_target"
        ;;
      "") ;;
      *) echo "refusing to remove unexpected target: $m6_backend_target" >&2 ;;
    esac
  fi
}

m6_preserve_logs() {
  [[ -n "${APPSTRUCT_E2E_LOG_DIR:-}" && -d "${temporary_root:-}" ]] || return 0
  local destination="$APPSTRUCT_E2E_LOG_DIR/$m6_suite" process_log
  mkdir -p -- "$destination"
  # Only process logs are retained. The temporary tree also contains environment files,
  # session cookies, rendered reports and test data, which are not diagnostic artifacts.
  for process_log in "$temporary_root"/*.log; do
    [[ -f "$process_log" && ! -L "$process_log" ]] || continue
    cp -- "$process_log" "$destination/"
  done
}

m6_prepare_fixture() {
  local fixture="$1"
  mkdir -p "$project"
  cp -R "$workspace/tests/fixtures/$fixture/." "$project/"
  # Spec fixtures share the same composition root as a newly created project.
  # A fixture may provide its own extensions instead.
  if [[ ! -f "$project/app/backend/Cargo.toml" ]]; then
    mkdir -p "$project/app/backend/src"
    cp "$workspace/crates/appstruct-cli/templates/common/app-backend.Cargo.toml" \
      "$project/app/backend/Cargo.toml"
    cp "$workspace/crates/appstruct-cli/templates/common/app-backend.lib.rs" \
      "$project/app/backend/src/lib.rs"
  fi
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
    --no-open --api-port "$api_port" --web-port "$web_port" >"$log" 2>&1 &
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
  m6_wait_for_url "http://127.0.0.1:$api_port/health/ready" "$dev_pid" "$log" \
    "${APPSTRUCT_E2E_STARTUP_TIMEOUT:-${1:-180}}" 1
  m6_wait_for_url "http://127.0.0.1:$web_port/" "$dev_pid" "$log" \
    "${APPSTRUCT_E2E_STARTUP_TIMEOUT:-${1:-180}}" 1
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

# Assert $APPSTRUCT_E2E_DATABASE_URL names a dedicated test database, then reset
# its public schema. The name must contain `e2e` or `test`.
m6_require_test_db() {
  local database_name
  database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
  case "$database_name" in
    *e2e*|*test*) ;;
    *)
      echo "refusing to reset database '$database_name'; use a dedicated database containing e2e or test in its name" >&2
      exit 2
      ;;
  esac
  psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public' >/dev/null
}

# Register an account over HTTP. `$api` must point at the dev server.
# Optional $4 password (default "correct-horse-battery"), optional $5 role:
# when set, the account's roles are updated via psql and the user id is
# printed on stdout.
m6_register() {
  local jar="$1" email="$2" output="$3" password="${4:-correct-horse-battery}" role="${5:-}" user_id
  curl --fail --silent --show-error -c "$jar" -b "$jar" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$email\",\"password\":\"$password\"}" \
    "$api/api/auth/register" >"$output"
  if [[ -n "$role" ]]; then
    user_id="$(jq -er '.user.id' "$output")"
    psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 \
      -c "UPDATE \"_appstruct_auth_accounts\" SET roles = '[\"$role\"]'::jsonb WHERE user_id = '$user_id'::uuid" \
      >/dev/null
    printf '%s' "$user_id"
  fi
}

m6_csrf_token() {
  awk '$6 == "appstruct_csrf" { print $7 }' "$1"
}

m6_create_org() {
  local jar="$1" name="$2" output="$3" csrf
  csrf="$(m6_csrf_token "$jar")"
  curl --fail --silent --show-error -c "$jar" -b "$jar" \
    -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
    -d "{\"name\":\"$name\"}" "$api/api/tenant/organizations" >"$output"
}

# assert_status <expected> <jar> <tenant> <method> <path> <output> [curl args...]
m6_assert_status() {
  local expected="$1" jar="$2" tenant="$3" method="$4" path="$5" output="$6"
  shift 6
  local status
  local -a request_args=(-X "$method")
  if [[ -n "$tenant" ]]; then
    request_args+=(-H "X-AppStruct-Tenant: $tenant")
  fi
  request_args+=("$@")
  status="$(curl --silent --show-error -o "$output" -w '%{http_code}' -b "$jar" \
    "${request_args[@]}" "$api$path")"
  if [[ "$status" != "$expected" ]]; then
    echo "expected HTTP $expected from $method $path for tenant '$tenant', got $status" >&2
    cat "$output" >&2
    tail -n 160 "${log:-/dev/null}" >&2 || true
    exit 1
  fi
}
