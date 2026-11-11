#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "$0")/.." && pwd)"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/appstruct-template-test.XXXXXX")"
trap 'rm -r "$temporary_root"' EXIT
mkdir -p "$temporary_root/bin"
cat >"$temporary_root/bin/cargo" <<'SH'
#!/usr/bin/env bash
echo "unexpected Cargo build" >&2
exit 99
SH
cat >"$temporary_root/bin/pnpm" <<'SH'
#!/usr/bin/env bash
exit 0
SH
cat >"$temporary_root/bin/appstruct" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$APPSTRUCT_TEST_CALLS"
if [[ "$3" == "new" ]]; then
  mkdir -p "$2/$4/generated/web"
fi
SH
chmod +x "$temporary_root/bin/"*
export PATH="$temporary_root/bin:$PATH"
export APPSTRUCT_TEST_CALLS="$temporary_root/calls"
APPSTRUCT_TEMPLATE_CLI="$temporary_root/bin/appstruct" APPSTRUCT_TEMPLATE=dashboard \
  bash "$workspace/scripts/run-template-build.sh"
[[ "$(wc -l <"$APPSTRUCT_TEST_CALLS" | tr -d ' ')" == 2 ]]
grep -q -- 'new ci-check --template dashboard' "$APPSTRUCT_TEST_CALLS"
grep -q -- '/ci-check generate' "$APPSTRUCT_TEST_CALLS"
if APPSTRUCT_TEMPLATE_CLI="$temporary_root/missing" bash "$workspace/scripts/run-template-build.sh"; then
  echo "missing prebuilt CLI was accepted" >&2
  exit 1
else
  [[ "$?" == 2 ]]
fi
echo "Prebuilt template CLI checks passed"
