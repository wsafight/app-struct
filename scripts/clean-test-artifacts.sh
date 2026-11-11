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
  "$workspace"/target/m6-e2e-*/
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
