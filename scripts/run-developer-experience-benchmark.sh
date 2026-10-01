#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cli="${APPSTRUCT_DX_CLI:-$workspace/target/debug/appstruct}"
output="${APPSTRUCT_DX_OUTPUT:-$workspace/output/benchmarks/developer-experience.json}"
benchmark_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-dx.XXXXXX")"
project="$benchmark_root/existing-postgres-admin"

cleanup() {
  [[ -e "$benchmark_root" ]] || return 0
  case "$benchmark_root" in
    "${TMPDIR:-/tmp}"/appstruct-dx.*) rm -r -- "$benchmark_root" ;;
    *) echo "Refusing to remove unexpected benchmark directory: $benchmark_root" >&2 ;;
  esac
}

handle_signal() {
  local status="$1"
  trap - EXIT INT TERM
  cleanup
  exit "$status"
}

trap cleanup EXIT
trap 'handle_signal 130' INT
trap 'handle_signal 143' TERM

for command in cargo node pnpm du; do
  command -v "$command" >/dev/null || {
    echo "Developer-experience benchmark requires $command" >&2
    exit 2
  }
done

integer_setting() {
  local name="$1" value="$2"
  case "$value" in
    ''|*[!0-9]*) echo "$name must be a positive integer" >&2; exit 2 ;;
  esac
  [[ "$value" -gt 0 ]] || { echo "$name must be greater than zero" >&2; exit 2; }
}

scaffold_budget_ms="${APPSTRUCT_DX_SCAFFOLD_BUDGET_MS:-5000}"
cold_generate_budget_ms="${APPSTRUCT_DX_COLD_GENERATE_BUDGET_MS:-30000}"
warm_generate_budget_ms="${APPSTRUCT_DX_WARM_GENERATE_BUDGET_MS:-3000}"
cold_build_budget_ms="${APPSTRUCT_DX_COLD_BUILD_BUDGET_MS:-900000}"
warm_build_budget_ms="${APPSTRUCT_DX_WARM_BUILD_BUDGET_MS:-120000}"
cache_budget_mb="${APPSTRUCT_DX_CACHE_BUDGET_MB:-8192}"
generated_budget_mb="${APPSTRUCT_DX_GENERATED_BUDGET_MB:-1024}"

for setting in \
  "APPSTRUCT_DX_SCAFFOLD_BUDGET_MS:$scaffold_budget_ms" \
  "APPSTRUCT_DX_COLD_GENERATE_BUDGET_MS:$cold_generate_budget_ms" \
  "APPSTRUCT_DX_WARM_GENERATE_BUDGET_MS:$warm_generate_budget_ms" \
  "APPSTRUCT_DX_COLD_BUILD_BUDGET_MS:$cold_build_budget_ms" \
  "APPSTRUCT_DX_WARM_BUILD_BUDGET_MS:$warm_build_budget_ms" \
  "APPSTRUCT_DX_CACHE_BUDGET_MB:$cache_budget_mb" \
  "APPSTRUCT_DX_GENERATED_BUDGET_MB:$generated_budget_mb"; do
  integer_setting "${setting%%:*}" "${setting#*:}"
done

if [[ ! -x "$cli" ]]; then
  cargo build --locked --manifest-path "$workspace/Cargo.toml" -p appstruct-cli
fi

measure() {
  local label="$1"
  shift
  local started finished
  started="$(node -e 'process.stdout.write(String(Date.now()))')"
  "$@" >/dev/null
  finished="$(node -e 'process.stdout.write(String(Date.now()))')"
  measured_ms=$((finished - started))
  echo "$label: ${measured_ms} ms"
}

measure "Scaffold" "$cli" --project "$benchmark_root" new existing-postgres-admin --template minimal
scaffold_ms="$measured_ms"
measure "Cold generate" "$cli" --project "$project" generate --timings
cold_generate_ms="$measured_ms"
measure "Warm generate" "$cli" --project "$project" generate --timings
warm_generate_ms="$measured_ms"
measure "Cold production build" "$cli" --project "$project" build
cold_build_ms="$measured_ms"
measure "Warm production build" "$cli" --project "$project" build
warm_build_ms="$measured_ms"

cache_kb="$(du -sk "$project/.appstruct/cache" | awk '{print $1}')"
generated_kb="$(du -sk "$project/generated" | awk '{print $1}')"
cache_mb=$(((cache_kb + 1023) / 1024))
generated_mb=$(((generated_kb + 1023) / 1024))
echo "Project cache: ${cache_mb} MiB"
echo "Generated tree: ${generated_mb} MiB"

mkdir -p "$(dirname "$output")"
node --input-type=module - \
  "$output" "$scaffold_ms" "$cold_generate_ms" "$warm_generate_ms" \
  "$cold_build_ms" "$warm_build_ms" "$cache_mb" "$generated_mb" <<'JS'
import { writeFileSync } from "node:fs";
import { arch, platform, release } from "node:os";

const [output, scaffold, coldGenerate, warmGenerate, coldBuild, warmBuild, cache, generated] =
  process.argv.slice(2);
const report = {
  schema_version: 1,
  measured_at: new Date().toISOString(),
  host: { platform: platform(), release: release(), arch: arch() },
  durations_ms: {
    scaffold: Number(scaffold),
    cold_generate: Number(coldGenerate),
    warm_generate: Number(warmGenerate),
    cold_build: Number(coldBuild),
    warm_build: Number(warmBuild),
  },
  sizes_mib: { project_cache: Number(cache), generated_tree: Number(generated) },
};
writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
JS

failed=0
check_budget() {
  local label="$1" actual="$2" budget="$3" unit="$4"
  if [[ "$actual" -gt "$budget" ]]; then
    echo "$label exceeded budget: $actual $unit > $budget $unit" >&2
    failed=1
  fi
}
check_budget "Scaffold" "$scaffold_ms" "$scaffold_budget_ms" ms
check_budget "Cold generate" "$cold_generate_ms" "$cold_generate_budget_ms" ms
check_budget "Warm generate" "$warm_generate_ms" "$warm_generate_budget_ms" ms
check_budget "Cold production build" "$cold_build_ms" "$cold_build_budget_ms" ms
check_budget "Warm production build" "$warm_build_ms" "$warm_build_budget_ms" ms
check_budget "Project cache" "$cache_mb" "$cache_budget_mb" MiB
check_budget "Generated tree" "$generated_mb" "$generated_budget_mb" MiB

[[ "$failed" -eq 0 ]] || exit 1
echo "Developer-experience budgets passed; report: $output"
