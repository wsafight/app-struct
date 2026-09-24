#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! -d "$1" ]]; then
  echo 'Usage: bash examples/public-demo/create.sh OUTPUT_PARENT_DIRECTORY' >&2
  exit 2
fi

output_parent="$(cd "$1" && pwd)"
destination="$output_parent/public-demo"
source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
binary="${APPSTRUCT_BIN:-appstruct}"

"$binary" --project "$output_parent" new public-demo --template saas
cp "$source_dir/appstruct.yaml" "$destination/appstruct.yaml"
cp "$source_dir/spec/work.yaml" "$destination/spec/work.yaml"
"$binary" --project "$destination" check
echo "Demo project ready at $destination"
