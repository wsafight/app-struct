#!/usr/bin/env bash
set -euo pipefail

version="" target="" archive_dir="" bin_dir="${HOME}/.local/bin"
usage() {
  echo 'Usage: bash install.sh --version VERSION [--bin-dir DIRECTORY] [--archive-dir DIRECTORY] [--target TARGET]'
}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version|--bin-dir|--archive-dir|--target)
      [[ $# -ge 2 && -n "$2" ]] || { usage >&2; exit 2; }
      case "$1" in
        --version) version="${2#v}" ;; --bin-dir) bin_dir="$2" ;;
        --archive-dir) archive_dir="$2" ;; --target) target="$2" ;;
      esac
      shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || { echo 'A pinned release version is required' >&2; exit 2; }
if [[ -z "$target" ]]; then
  case "$(uname -s):$(uname -m)" in
    Darwin:arm64) target=aarch64-apple-darwin ;;
    Darwin:x86_64) target=x86_64-apple-darwin ;;
    Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
    Linux:x86_64) target=x86_64-unknown-linux-musl ;;
    *) echo 'Unsupported platform; use a published archive or build from source' >&2; exit 2 ;;
  esac
fi
case "$target" in
  aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-musl|x86_64-unknown-linux-musl|aarch64-unknown-linux-gnu|x86_64-unknown-linux-gnu) ;;
  *) echo 'Unsupported release target' >&2; exit 2 ;;
esac
archive="appstruct-$version-$target.tar.gz"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-install.XXXXXX")"
staged=""
cleanup() {
  [[ -z "$staged" ]] || rm -f "$staged"
  rm -r "$temporary"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ -n "$archive_dir" ]]; then
  cp "$archive_dir/$archive" "$archive_dir/$archive.sha256" "$temporary/"
else
  for file in "$archive" "$archive.sha256"; do
    curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
      --connect-timeout 15 --max-time 300 --retry 2 \
      "https://github.com/wsafight/app-struct/releases/download/v$version/$file" -o "$temporary/$file"
  done
fi
expected="$(awk 'NR == 1 {print $1}' "$temporary/$archive.sha256")"
[[ "$expected" =~ ^[a-fA-F0-9]{64}$ ]] || { echo 'Invalid release checksum' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$temporary/$archive" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$temporary/$archive" | awk '{print $1}')"
fi
[[ "$actual" == "$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')" ]] || { echo 'Release checksum mismatch' >&2; exit 1; }
mkdir -p "$bin_dir"
[[ ! -d "$bin_dir/appstruct" ]] || { echo 'Install destination is a directory' >&2; exit 1; }
staged="$(mktemp "$bin_dir/.appstruct-install.XXXXXX")"
# Stream only the expected archive entry; archive paths are never extracted to disk.
tar -xOzf "$temporary/$archive" "appstruct-$version-$target/appstruct" >"$staged"
[[ -s "$staged" ]] || { echo 'Release archive has no binary' >&2; exit 1; }
chmod 0755 "$staged"
mv -f "$staged" "$bin_dir/appstruct"
staged=""
echo "Installed AppStruct $version to $bin_dir/appstruct"
echo "Ensure $bin_dir is on PATH."
