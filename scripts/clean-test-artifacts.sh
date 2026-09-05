#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "$0")/.." && pwd)"

if [[ "${1:-}" == "--all" ]]; then
  if [[ $# -ne 1 ]]; then
    echo "usage: scripts/clean-test-artifacts.sh [--all]" >&2
    exit 2
  fi
  cargo clean --manifest-path "$workspace/Cargo.toml"
  exit 0
fi
if [[ $# -ne 0 ]]; then
  echo "usage: scripts/clean-test-artifacts.sh [--all]" >&2
  exit 2
fi

targets=(
  "$workspace/target/appstruct-generated-tests"
  "$workspace/target/llvm-cov-target"
  "$workspace/target/package"
  "$workspace/target/m6-e2e-audit"
  "$workspace/target/m6-e2e-bulk"
  "$workspace/target/m6-e2e-file"
  "$workspace/target/m6-e2e-jobs"
  "$workspace/target/m6-e2e-mail"
  "$workspace/target/m6-e2e-saas"
  "$workspace/target/m6-e2e-tenant"
  "$workspace/target/m6-e2e-operations"
  "$workspace/target/m6-e2e-chromium-report"
  "$workspace/target/m6-e2e-benchmark"
  "$workspace/target/m6-e2e-deployment"
)

removed=0
for target in "${targets[@]}"; do
  if [[ -d "$target" ]]; then
    rm -r -- "$target"
    echo "Removed ${target#"$workspace/"}"
    removed=$((removed + 1))
  fi
done

if [[ $removed -eq 0 ]]; then
  echo "No disposable test targets found"
fi
