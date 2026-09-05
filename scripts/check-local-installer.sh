#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$workspace/target/debug/appstruct}"
version="$("$binary" --version)"
version="${version#appstruct }"
target="$(rustc -vV | awk '/^host: / {print $2}')"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-local-archive.XXXXXX")"
trap 'rm -r "$temporary"' EXIT
stem="appstruct-$version-$target"
mkdir "$temporary/$stem"
cp "$binary" "$temporary/$stem/appstruct"
cp "$workspace/README.md" "$workspace/LICENSE-MIT" "$workspace/LICENSE-APACHE" "$temporary/$stem/"
tar -C "$temporary" -czf "$temporary/$stem.tar.gz" "$stem"
(cd "$temporary" && shasum -a 256 "$stem.tar.gz" >"$stem.tar.gz.sha256")
bash "$workspace/scripts/test-installer.sh" "$temporary" "$version" "$target"
