#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init saas-product 0 55850 saas-product-browser false
m6_build_cli

# Copy the tracked example without relying on local generated files or node_modules.
while IFS= read -r -d '' source_file; do
  destination="$temporary_root/$source_file"
  mkdir -p "$(dirname "$destination")"
  cp "$workspace/$source_file" "$destination"
done < <(git -C "$workspace" ls-files -z examples/saas-demo)
project="$temporary_root/examples/saas-demo"

target/debug/appstruct --project "$project" generate
pnpm --dir "$project/generated/web" install --frozen-lockfile
# Custom modules under app/web resolve their imports from the project root.
ln -s generated/web/node_modules "$project/node_modules"
pnpm --dir "$project/generated/web" run typecheck
pnpm --dir "$project/generated/web" run build

set -m
pnpm --dir "$project/generated/web" exec rsbuild dev \
  --host 127.0.0.1 --port "$web_port" >"$log" 2>&1 &
dev_pid=$!
set +m

m6_wait_for_url "http://127.0.0.1:$web_port/" "$dev_pid" "$log" \
  "${APPSTRUCT_E2E_STARTUP_TIMEOUT:-120}" 1

pnpm install --frozen-lockfile
APPSTRUCT_E2E_SAAS_PRODUCT=1 PLAYWRIGHT_BASE_URL="http://127.0.0.1:$web_port" \
  pnpm exec playwright test tests/e2e/saas-product.spec.ts

echo "SaaS product desktop and mobile browser E2E passed"
