#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/m6-e2e-common.sh"
m6_init operations 33900 55900 operations-e2e

m6_require_test_db

mkdir -p "$project"
cp -R "$workspace/examples/operations-demo/." "$project/"
cp "$workspace/tests/e2e/operations.external.yaml" "$project/appstruct.yaml"
cp "$workspace/tests/e2e-operations/backend.Cargo.toml" "$project/app/backend/Cargo.toml"
cp "$workspace/tests/e2e-operations/project.env" "$project/.env"
mkdir -p "$project/.appstruct/cache" "$m6_backend_target"
ln -s "$m6_backend_target" "$project/.appstruct/cache/backend-target"
m6_build_cli

job_gate="$project/.appstruct/job-worker.open"
m6_start_dev
m6_wait_for_dev 240
m6_wait_for_url "http://127.0.0.1:$web_port/" "$dev_pid" "$log" 240 1

api="http://127.0.0.1:$api_port"
password="AppStruct-Operations-E2E-2026"
operator_jar="$temporary_root/operator.cookies"
supplier_jar="$temporary_root/supplier.cookies"
auditor_jar="$temporary_root/auditor.cookies"
admin_jar="$temporary_root/admin.cookies"

operator_id="$(m6_register "$operator_jar" "operator@operations.example.test" "$temporary_root/operator.json" "$password" operator)"
supplier_id="$(m6_register "$supplier_jar" "supplier@operations.example.test" "$temporary_root/supplier.json" "$password" supplier)"
auditor_id="$(m6_register "$auditor_jar" "auditor@operations.example.test" "$temporary_root/auditor.json" "$password" auditor)"
admin_id="$(m6_register "$admin_jar" "admin@operations.example.test" "$temporary_root/admin.json" "$password" admin)"

operator_csrf="$(m6_csrf_token "$operator_jar")"
m6_create_org "$operator_jar" "Operations Alpha" "$temporary_root/alpha.json"
m6_create_org "$operator_jar" "Operations Beta" "$temporary_root/beta.json"
alpha="$(jq -er '.id' "$temporary_root/alpha.json")"
beta="$(jq -er '.id' "$temporary_root/beta.json")"
[[ "$alpha" != "$beta" ]]

for user_id in "$supplier_id" "$auditor_id" "$admin_id"; do
  psql "$APPSTRUCT_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -c "INSERT INTO \"_appstruct_tenant_memberships\" (organization_id, user_id, role, created_at) VALUES ('$alpha', '$user_id', 'member', CURRENT_TIMESTAMP)" \
    >/dev/null
done

curl --fail --silent --show-error -b "$operator_jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "X-AppStruct-Tenant: $alpha" \
  -d '{"sku":"OPS-001","name":"Operations sample","unit":"each","active":true}' \
  "$api/api/products/" >"$temporary_root/product.json"
product_id="$(jq -er '.id' "$temporary_root/product.json")"

curl --fail --silent --show-error -b "$operator_jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "X-AppStruct-Tenant: $alpha" \
  -d "{\"product_id\":\"$product_id\",\"location\":\"A-01\",\"quantity_on_hand\":50,\"reorder_level\":10}" \
  "$api/api/inventory/" >"$temporary_root/inventory.json"

curl --fail --silent --show-error -b "$operator_jar" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "X-AppStruct-Tenant: $alpha" \
  -d "{\"number\":\"OPS-2026-0001\",\"owner_id\":\"$operator_id\",\"notes\":\"Deterministic operations scenario\"}" \
  "$api/api/orders/" >"$temporary_root/order.json"
order_id="$(jq -er '.id' "$temporary_root/order.json")"
revision="$(jq -er '.revision' "$temporary_root/order.json")"
jq -e --arg tenant "$alpha" '.tenant_id == $tenant' "$temporary_root/order.json" >/dev/null

source "$workspace/scripts/operations-aggregate-checks.sh"

m6_assert_status 404 "$operator_jar" "$beta" GET "/api/orders/$order_id" "$temporary_root/cross-tenant.json"
for audience in owner-other-tenant supplier-same-tenant; do
  if [[ "$audience" == owner-other-tenant ]]; then
    scope_jar="$operator_jar" scope_tenant="$beta"
  else
    scope_jar="$supplier_jar" scope_tenant="$alpha"
  fi
  m6_assert_status 200 "$scope_jar" "$scope_tenant" GET '/api/orders/' "$temporary_root/scoped-list.json"
  jq -e '.data == [] and .meta.total == 0' "$temporary_root/scoped-list.json" >/dev/null
  m6_assert_status 200 "$scope_jar" "$scope_tenant" GET "/api/orders/_lookup?ids=$order_id" "$temporary_root/scoped-lookup.json"
  jq -e '. == []' "$temporary_root/scoped-lookup.json" >/dev/null
  m6_assert_status 200 "$scope_jar" "$scope_tenant" GET '/api/orders/_aggregate?metrics=count' "$temporary_root/scoped-count.json"
  jq -e '.data[0].count == 0' "$temporary_root/scoped-count.json" >/dev/null
  m6_assert_status 200 "$scope_jar" "$scope_tenant" GET '/api/orders/_export.csv' "$temporary_root/scoped-export.csv"
  [[ "$(wc -l < "$temporary_root/scoped-export.csv" | tr -d ' ')" == 1 ]]
  m6_assert_status 404 "$scope_jar" "$scope_tenant" GET "/api/orders/$order_id/_transitions" "$temporary_root/scoped-transitions.json"
  m6_assert_status 404 "$scope_jar" "$scope_tenant" GET "/api/orders/$order_id/_aggregates/lines" "$temporary_root/scoped-collection.json"
done
m6_assert_status 200 "$operator_jar" "$alpha" GET "/api/orders/_lookup?ids=$order_id" "$temporary_root/own-lookup.json"
jq -e --arg id "$order_id" 'length == 1 and .[0].id == $id' "$temporary_root/own-lookup.json" >/dev/null
m6_assert_status 200 "$operator_jar" "$beta" GET '/api/order_lines/?filter%5Border.status%5D=draft' "$temporary_root/scoped-relation.json"
jq -e '.data == [] and .meta.total == 0' "$temporary_root/scoped-relation.json" >/dev/null
supplier_csrf="$(m6_csrf_token "$supplier_jar")"
m6_assert_status 404 "$supplier_jar" "$alpha" POST "/api/orders/$order_id/_transitions/approve" \
  "$temporary_root/supplier-transition.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $supplier_csrf" \
  -H "If-Match: \"rev-$revision\"" -d '{}'

m6_assert_status 200 "$operator_jar" "$alpha" POST "/api/orders/$order_id/_transitions/submit" \
  "$temporary_root/submitted.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "If-Match: \"rev-$revision\"" -d '{}'
revision="$(jq -er '.revision' "$temporary_root/submitted.json")"

auditor_csrf="$(m6_csrf_token "$auditor_jar")"
m6_assert_status 200 "$auditor_jar" "$alpha" POST "/api/orders/$order_id/_transitions/approve" \
  "$temporary_root/approved.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $auditor_csrf" \
  -H "If-Match: \"rev-$revision\"" -d '{}'
revision="$(jq -er '.revision' "$temporary_root/approved.json")"
jq -e '.status == "approved"' "$temporary_root/approved.json" >/dev/null
m6_assert_status 409 "$operator_jar" "$alpha" POST "/api/orders/$order_id/_aggregates/lines" \
  "$temporary_root/frozen-collection.json" -H "Content-Type: application/json" \
  -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" \
  --data-binary "@$temporary_root/create-lines.json"

m6_assert_status 200 "$operator_jar" "$alpha" PATCH "/api/orders/$order_id" \
  "$temporary_root/first-update.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "If-Match: \"rev-$revision\"" -d '{"notes":"Revision winner"}'
current_revision="$(jq -er '.revision' "$temporary_root/first-update.json")"
m6_assert_status 412 "$operator_jar" "$alpha" PATCH "/api/orders/$order_id" \
  "$temporary_root/revision-conflict.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "If-Match: \"rev-$revision\"" -d '{"notes":"Revision loser"}'
jq -e '.error.code == "CONCURRENT_MODIFICATION"' "$temporary_root/revision-conflict.json" >/dev/null

jq -cn --arg id "$order_id" --argjson revision "$current_revision" '{
  ids: [$id], patch: {status: "draft"}, expected_revisions: {($id): $revision}
}' >"$temporary_root/bulk-workflow.json"
m6_assert_status 200 "$operator_jar" "$alpha" PATCH "/api/orders/_bulk" \
  "$temporary_root/bulk-workflow-result.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  --data-binary "@$temporary_root/bulk-workflow.json"
m6_assert_status 200 "$operator_jar" "$alpha" GET "/api/orders/$order_id" "$temporary_root/after-bulk.json"
jq -e '.status == "approved"' "$temporary_root/after-bulk.json" >/dev/null

csv_payload=$'number,owner,status\nOPS-BYPASS,'"$operator_id"$',draft\n'
m6_assert_status 400 "$operator_jar" "$alpha" POST "/api/orders/_import.csv" \
  "$temporary_root/csv-workflow.json" \
  -H "Content-Type: text/csv" -H "X-CSRF-Token: $operator_csrf" --data-binary "$csv_payload"

comment_payload='{"body":"Approved for fulfillment","attachment":{"name":"approval.txt","content_type":"text/plain","content_base64":"YXBwcm92ZWQ="}}'
m6_assert_status 200 "$operator_jar" "$alpha" POST "/api/activity/orders/$order_id/comments" \
  "$temporary_root/comment.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -d "$comment_payload"
entry_id="$(jq -er '.id' "$temporary_root/comment.json")"
m6_assert_status 200 "$operator_jar" "$alpha" GET "/api/activity/orders/$order_id?limit=20" "$temporary_root/activity.json"
jq -e --arg actor "$operator_id" --arg tenant "$alpha" '
  ([.data[] | select(.kind == "comment" and .body == "Approved for fulfillment" and .actor_id == $actor and .tenant_id == $tenant)] | length) == 1 and
  ([.data[] | select(.event == "created")] | length) == 1 and
  ([.data[] | select(.event == "workflow.submit")] | length) == 1 and
  ([.data[] | select(.event == "workflow.approve")] | length) == 1
' "$temporary_root/activity.json" >/dev/null
m6_assert_status 200 "$operator_jar" "$alpha" GET "/api/activity/orders/$order_id/$entry_id/attachment" "$temporary_root/approval.txt"
[[ "$(cat "$temporary_root/approval.txt")" == "approved" ]]
m6_assert_status 404 "$supplier_jar" "$alpha" GET "/api/activity/orders/$order_id/$entry_id/attachment" "$temporary_root/supplier-attachment.json"
m6_assert_status 404 "$operator_jar" "$beta" GET "/api/activity/orders/$order_id/$entry_id/attachment" "$temporary_root/cross-attachment.json"
m6_assert_status 404 "$operator_jar" "$beta" GET "/api/activity/orders/$order_id?limit=20" "$temporary_root/cross-activity.json"

report_body="{\"data\":{\"order_id\":\"$order_id\",\"order_number\":\"OPS-CANCEL\"}}"
m6_assert_status 202 "$operator_jar" "$alpha" POST "/api/reports/templates/order-summary/runs" \
  "$temporary_root/cancel-run.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "Idempotency-Key: operations-cancel" -d "$report_body"
cancel_id="$(jq -er '.id' "$temporary_root/cancel-run.json")"
m6_assert_status 200 "$operator_jar" "$alpha" POST "/api/reports/runs/$cancel_id/cancel" \
  "$temporary_root/cancelled.json" -H "X-CSRF-Token: $operator_csrf"
jq -e '.stage == "cancelled"' "$temporary_root/cancelled.json" >/dev/null

report_body="{\"data\":{\"order_id\":\"$order_id\",\"order_number\":\"OPS-RETRY-ONCE\"}}"
m6_assert_status 202 "$operator_jar" "$alpha" POST "/api/reports/templates/order-summary/runs" \
  "$temporary_root/retry-run.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" \
  -H "Idempotency-Key: operations-retry" -d "$report_body"
report_id="$(jq -er '.id' "$temporary_root/retry-run.json")"
touch "$job_gate"

report_stage=""
for _ in {1..160}; do
  m6_assert_status 200 "$operator_jar" "$alpha" GET "/api/reports/runs/$report_id" "$temporary_root/report-status.json"
  report_stage="$(jq -er '.stage' "$temporary_root/report-status.json")"
  if [[ "$report_stage" == "succeeded" ]]; then break; fi
  sleep 0.1
done
if [[ "$report_stage" != "succeeded" ]]; then
  echo "report did not succeed after its injected retry" >&2
  cat "$temporary_root/report-status.json" >&2
  exit 1
fi
attempts="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT attempts FROM \"_appstruct_jobs\" WHERE id = (SELECT execution_job_id FROM \"_appstruct_report_runs\" WHERE id = '$report_id')")"
[[ "$attempts" == "2" ]]
m6_assert_status 200 "$operator_jar" "$alpha" GET "/api/reports/runs/$report_id/download" "$temporary_root/order.pdf"
[[ "$(head -c 8 "$temporary_root/order.pdf")" == "%PDF-1.4" ]]
m6_assert_status 404 "$operator_jar" "$beta" GET "/api/reports/runs/$report_id/download" "$temporary_root/cross-report.json"
m6_assert_status 403 "$supplier_jar" "$alpha" GET "/api/reports/runs/$report_id/download" "$temporary_root/supplier-report.json"

m6_assert_status 200 "$auditor_jar" "$alpha" GET "/api/audit/events?page_size=100" "$temporary_root/audit.json"
jq -e --arg tenant "$alpha" --arg operator "$operator_id" --arg auditor "$auditor_id" '
  ([.data[] | select(.operation == "workflow.submit" and .tenant_id == $tenant and .actor_id == $operator)] | length) == 1 and
  ([.data[] | select(.operation == "workflow.approve" and .tenant_id == $tenant and .actor_id == $auditor)] | length) == 1 and
  ([.data[] | select(.operation == "report.create" and .tenant_id == $tenant and .actor_id == $operator)] | length) == 2 and
  ([.data[] | select(.operation == "report.download" and .tenant_id == $tenant and .actor_id == $operator)] | length) == 1
' "$temporary_root/audit.json" >/dev/null

binding_count="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM \"_appstruct_report_runs\" r JOIN \"_appstruct_jobs\" j ON j.id = r.execution_job_id JOIN \"_appstruct_files\" f ON f.id = r.result_file_id WHERE r.id = '$report_id' AND r.tenant_id = '$alpha' AND r.actor_id = '$operator_id' AND j.tenant_id = r.tenant_id AND f.tenant_id = r.tenant_id")"
[[ "$binding_count" == "1" ]]

m6_typecheck_web
pnpm --dir "$project/generated/web" run format:check
pnpm --dir "$project/generated/web" run lint
pnpm --dir "$project/generated/web" run test
pnpm --dir "$project/generated/web" run build
m6_run_playwright playwright.operations.config.ts \
  "OPERATIONS_E2E_ORDER_ID=$order_id" \
  "OPERATIONS_E2E_PRODUCT_ID=$product_id" \
  "OPERATIONS_E2E_OPERATOR_ID=$operator_id" \
  "OPERATIONS_E2E_API=$api" \
  "OPERATIONS_E2E_ORDER_LINE_ID=$order_line_id"
echo "Operations PostgreSQL and browser E2E passed"
