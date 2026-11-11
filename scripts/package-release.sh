#!/usr/bin/env bash
set -euo pipefail

version="${1:?version required}"
target="${2:?target required}"
binary="${3:?binary path required}"
output_dir="${4:?output directory required}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || { echo 'Invalid release version' >&2; exit 2; }
case "$target" in
  aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-gnu|x86_64-unknown-linux-gnu) ;;
  *) echo 'Unsupported release target' >&2; exit 2 ;;
esac
[[ -f "$binary" && -x "$binary" ]] || { echo 'Release binary is missing or not executable' >&2; exit 1; }
[[ "$("$binary" --version)" == "appstruct $version" ]] || { echo 'Release binary version does not match' >&2; exit 1; }

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
stem="appstruct-$version-$target"
archive="$output_dir/$stem.tar.gz"
[[ ! -e "$archive" && ! -e "$archive.sha256" ]] || { echo 'Release archive already exists' >&2; exit 1; }
temporary="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-package.XXXXXX")"
trap 'rm -r "$temporary"' EXIT
mkdir "$temporary/$stem"
install -m 0755 "$binary" "$temporary/$stem/appstruct"
cp "$workspace/README.md" "$workspace/LICENSE-MIT" "$workspace/LICENSE-APACHE" "$temporary/$stem/"
tar -C "$temporary" -czf "$archive" "$stem"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$output_dir" && sha256sum "$stem.tar.gz" >"$stem.tar.gz.sha256")
else
  (cd "$output_dir" && shasum -a 256 "$stem.tar.gz" >"$stem.tar.gz.sha256")
fi
echo "Packaged $archive"
