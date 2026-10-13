#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-cleanup-test.XXXXXX")"
trap 'rm -r -- "$test_root"' EXIT
mkdir -p "$test_root/tmp with spaces"

for expected_status in 0 7; do
  status=0
  env TMPDIR="$test_root/tmp with spaces" \
    APPSTRUCT_E2E_DATABASE_URL=postgresql://localhost/unused_test \
    APPSTRUCT_E2E_LOG_DIR="$test_root/logs" \
    APPSTRUCT_KEEP_TEST_TARGETS=0 \
    bash -s -- "$workspace" "$test_root" "$expected_status" <<'SH' || status=$?
set -euo pipefail
source "$1/scripts/m6-e2e-common.sh"
m6_init "cleanup-test-$$" 3000 5173
printf '%s\n' "$temporary_root" >"$2/temporary-path"
printf '%s\n' "$m6_backend_target" >"$2/target-path"
printf '%s\n' "$m6_suite" >"$2/suite"
mkdir -p "$m6_backend_target" "$project"
printf 'API startup log\n' >"$log"
printf 'renderer log\n' >"$temporary_root/renderer.log"
printf 'secret\n' >"$temporary_root/.env"
printf 'cookie\n' >"$temporary_root/session.cookies"
ln -s "$temporary_root/.env" "$temporary_root/linked.log"
m6_cleanup_extra() { printf 'shutdown log\n' >"$temporary_root/shutdown.log"; }
exit "$3"
SH
  [[ "$status" == "$expected_status" ]]
  [[ ! -e "$(cat "$test_root/temporary-path")" ]]
  [[ ! -e "$(cat "$test_root/target-path")" ]]
  logs="$test_root/logs/$(cat "$test_root/suite")"
  [[ "$(cat "$logs/dev.log")" == 'API startup log' ]]
  [[ "$(cat "$logs/renderer.log")" == 'renderer log' ]]
  [[ "$(cat "$logs/shutdown.log")" == 'shutdown log' ]]
  [[ ! -e "$logs/.env" && ! -e "$logs/session.cookies" && ! -e "$logs/linked.log" ]]
done

echo 'E2E cleanup retains process logs, preserves exit status and removes temporary artifacts'
