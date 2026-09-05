#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init file 33700 55700
storage="$temporary_root/storage"
mkdir -p "$storage"
m6_prepare_fixture m6-file-project
m6_start_dev "APPSTRUCT_FILE_ROOT=$storage"
m6_wait_for_dev

api="http://127.0.0.1:$api_port"
jar="$temporary_root/file.cookies"
email="file-$RANDOM-$$@example.com"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$email\",\"password\":\"correct-horse-battery\"}" \
  "$api/api/auth/register" >"$temporary_root/user.json"
csrf="$(awk '$6 == "appstruct_csrf" { print $7 }' "$jar")"
curl --fail --silent --show-error -c "$jar" -b "$jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $csrf" \
  -d '{"name":"File Tenant"}' "$api/api/tenant/organizations" \
  >"$temporary_root/tenant.json"
tenant="$(jq -er '.id' "$temporary_root/tenant.json")"

file_id="$(APPSTRUCT_FILE_ROOT="$storage" DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" \
  TENANT_ID="$tenant" cargo run --quiet --manifest-path "$project/app/file-e2e/Cargo.toml")"
curl --fail --silent --show-error -b "$jar" \
  "$api/api/admin/files?search=summary&page=1&page_size=10" \
  >"$temporary_root/admin-files.json"
jq -e --arg id "$file_id" --arg tenant "$tenant" '
  .meta == {page: 1, page_size: 10, total: 1} and .total_bytes > 0 and
  (.data | length) == 1 and .data[0].id == $id and
  .data[0].object_key == "reports/summary.json" and .data[0].tenant_id == $tenant
' "$temporary_root/admin-files.json" >/dev/null
curl --fail --silent --show-error -b "$jar" \
  "$api/api/admin/files/$file_id" >"$temporary_root/admin-file-detail.json"
jq -e '
  .original_name == "summary.json" and .content_type == "application/json" and
  (.checksum | test("^[0-9a-f]{64}$")) and (has("content") | not)
' "$temporary_root/admin-file-detail.json" >/dev/null
m6_typecheck_web
echo "File PostgreSQL, local object-storage, and admin E2E passed"
