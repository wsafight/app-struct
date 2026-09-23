#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$workspace/target/debug/appstruct}"
version="$("$binary" --version)"
version="${version#appstruct }"
target="$(rustc -vV | awk '/^host: / {print $2}')"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-local-archive.XXXXXX")"
trap 'rm -r "$temporary"' EXIT
bash "$workspace/scripts/package-release.sh" "$version" "$target" "$binary" "$temporary"
stem="appstruct-$version-$target"
for member in appstruct README.md LICENSE-MIT LICENSE-APACHE; do
  tar -tzf "$temporary/$stem.tar.gz" "$stem/$member" >/dev/null
done
bash "$workspace/scripts/test-installer.sh" "$temporary" "$version" "$target"
