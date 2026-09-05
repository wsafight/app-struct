#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive_dir="${1:?archive directory required}"
version="${2:?version required}"
target="${3:?target required}"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-installer-test.XXXXXX")"
trap 'rm -r "$temporary"' EXIT
args=(--version "$version" --target "$target" --archive-dir "$archive_dir" --bin-dir "$temporary/bin with spaces")
bash "$workspace/scripts/install.sh" "${args[@]}"
[[ "$("$temporary/bin with spaces/appstruct" --version)" == "appstruct $version" ]]
cp "$temporary/bin with spaces/appstruct" "$temporary/original"
bash "$workspace/scripts/install.sh" "${args[@]}"
cmp "$temporary/original" "$temporary/bin with spaces/appstruct"
mkdir "$temporary/corrupt"
cp "$archive_dir/appstruct-$version-$target.tar.gz" "$archive_dir/appstruct-$version-$target.tar.gz.sha256" "$temporary/corrupt/"
printf 'corrupt' >>"$temporary/corrupt/appstruct-$version-$target.tar.gz"
if bash "$workspace/scripts/install.sh" "${args[@]}" --archive-dir "$temporary/corrupt"; then
  echo 'Installer accepted a corrupt archive' >&2
  exit 1
fi
cmp "$temporary/original" "$temporary/bin with spaces/appstruct"
if bash "$workspace/scripts/install.sh" --version '../../invalid' --bin-dir "$temporary/bin with spaces"; then
  echo 'Installer accepted an invalid version' >&2
  exit 1
fi
echo 'Installer checks passed: install, replace, checksum rejection and original binary preservation'
