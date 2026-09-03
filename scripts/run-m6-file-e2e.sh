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

APPSTRUCT_FILE_ROOT="$storage" DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" TENANT_ID="$tenant" \
  cargo run --quiet --manifest-path "$project/app/file-e2e/Cargo.toml"
m6_typecheck_web
echo "File PostgreSQL and local object-storage E2E passed"
