#!/usr/bin/env bash
set -euo pipefail
mode="${1:---docker}"
[[ "$mode" == --docker || "$mode" == --native ]] || { echo 'Usage: run-deployment-e2e.sh [--docker|--native]' >&2; exit 2; }
if [[ "$mode" == --docker ]]; then docker compose version >/dev/null; fi
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init deployment 34200 56200 deployment-smoke
api_pid="" proxy_pid="" compose_started=""
compose=(docker compose --project-directory "$project" --project-name "appstruct-deployment-$web_port"
  -f "$project/compose.production.yaml" -f "$workspace/tests/deployment/compose.yaml")
m6_cleanup_extra() {
  m6_stop_process "$proxy_pid" TERM || true
  m6_stop_process "$api_pid" TERM || true
  if [[ -n "$compose_started" ]]; then "${compose[@]}" down --volumes --remove-orphans; fi
}
database_name="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
case "$database_name" in *test*|*e2e*) ;; *) echo 'Dedicated test database required' >&2; exit 2 ;; esac
psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public' >/dev/null
m6_build_cli
cli="$workspace/target/debug/appstruct"
"$cli" --project "$temporary_root" new deployment-smoke --template minimal
mkdir -p "$project/.appstruct/cache" "$m6_backend_target"
ln -s "$m6_backend_target" "$project/.appstruct/cache/backend-target"
env -u VITE_API_URL DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" build
env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" migrate dev --accept
env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" migrate apply
env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" "$cli" --project "$project" migrate status
"$cli" --project "$project" generate --check
if [[ "$mode" == --docker ]]; then
  node --input-type=module - "$project/.env.production" <<'JS'
import { writeFileSync } from 'node:fs';
const url = new URL(process.env.APPSTRUCT_E2E_DATABASE_URL);
if (['localhost', '127.0.0.1'].includes(url.hostname)) url.hostname = 'host.docker.internal';
writeFileSync(process.argv[2], `DATABASE_URL=${url}\n`, { mode: 0o600 });
JS
  export APPSTRUCT_WEB_PORT="$web_port"
  compose_started=1
  "${compose[@]}" up --build --wait --wait-timeout 120
else
  env DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL" APPSTRUCT_BIND="127.0.0.1:$api_port" \
    "$m6_backend_target/release/appstruct-generated-server" >"$temporary_root/api.log" 2>&1 &
  api_pid=$!
  m6_wait_for_url "http://127.0.0.1:$api_port/health/ready" "$api_pid" "$temporary_root/api.log" 30
  node "$workspace/tests/deployment/native-proxy.mjs" "$project/generated/web/dist" "http://127.0.0.1:$api_port" "$web_port" >"$temporary_root/proxy.log" 2>&1 &
  proxy_pid=$!
  m6_wait_for_url "http://127.0.0.1:$web_port/health/ready" "$proxy_pid" "$temporary_root/proxy.log" 30
fi
node "$project/deploy/smoke.mjs" "http://127.0.0.1:$web_port"
pnpm --dir "$workspace" install --frozen-lockfile
node "$workspace/tests/deployment/browser.mjs" "http://127.0.0.1:$web_port" "$workspace/output/deployment"
echo "First-deployment checks passed ($mode)"
