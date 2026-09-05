#!/usr/bin/env bash
# Sourced by run-operations-e2e.sh after creating the parent and product.

collection_path="/api/orders/$order_id/_aggregates/lines"
jq -cn --arg product "$product_id" '{creates: [{key: "first", input: {product_id: $product, quantity: 2, unit_price: "19.95", currency: "CNY"}}]}' >"$temporary_root/create-lines.json"

assert_status 428 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/missing-parent-version.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" --data-binary "@$temporary_root/create-lines.json"
assert_status 200 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/created-lines.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/create-lines.json"
order_line_id="$(jq -er '.created.first' "$temporary_root/created-lines.json")"
old_revision="$revision"
revision="$(jq -er '.parent.revision' "$temporary_root/created-lines.json")"
jq -e '.rows | length == 1' "$temporary_root/created-lines.json" >/dev/null
assert_status 412 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/stale-parent.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$old_revision\"" --data-binary "@$temporary_root/create-lines.json"

for endpoint in '/' '/_bulk' '/_import.csv' "/$order_line_id"; do
  for method in POST PATCH DELETE; do
    if [[ "$endpoint" == '/_bulk' && "$method" == POST ]] || [[ "$endpoint" == '/_import.csv' && "$method" != POST ]] || [[ "$endpoint" == '/' && "$method" != POST ]] || [[ "$endpoint" == "/$order_line_id" && "$method" == POST ]]; then continue; fi
    assert_status 405 "$operator_jar" "$alpha" "$method" "/api/order_lines$endpoint" "$temporary_root/owned-write.json" \
      -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H 'If-Match: "rev-1"' -d '{}'
  done
done

audit_before="$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c 'SELECT count(*) FROM _appstruct_audit_events')"
jq -cn --arg id "$order_line_id" --arg product "$product_id" '{deletes: [{id: $id, revision: 1}], creates: [{key: "invalid", input: {product_id: $product, quantity: 0, unit_price: "1.00", currency: "CNY"}}]}' >"$temporary_root/rollback-lines.json"
assert_status 422 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/rollback-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/rollback-lines.json"
jq -e '.error.fields[0].field == "creates.invalid.quantity"' "$temporary_root/rollback-response.json" >/dev/null
[[ "$(psql "$APPSTRUCT_E2E_DATABASE_URL" -At -c 'SELECT count(*) FROM _appstruct_audit_events')" == "$audit_before" ]]
assert_status 200 "$operator_jar" "$alpha" GET "$collection_path" "$temporary_root/after-rollback.json"
jq -e --arg id "$order_line_id" --argjson revision "$revision" '.parent.revision == $revision and (.rows | length == 1) and .rows[0].id == $id and .rows[0].quantity == 2 and .rows[0].revision == 1' "$temporary_root/after-rollback.json" >/dev/null

jq -cn --arg id "$order_line_id" '{updates: [{id: $id, revision: 99, input: {quantity: 3}}]}' >"$temporary_root/stale-child.json"
assert_status 412 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/stale-child-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/stale-child.json"

assert_status 201 "$operator_jar" "$beta" POST '/api/products/' "$temporary_root/beta-product.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -d '{"sku":"BETA-ONLY","name":"Beta product","unit":"each","active":true}'
beta_product="$(jq -er '.id' "$temporary_root/beta-product.json")"
jq --arg product "$beta_product" '.creates[0].input.product_id = $product' "$temporary_root/create-lines.json" >"$temporary_root/foreign-relation.json"
assert_status 404 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/foreign-relation-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/foreign-relation.json"

assert_status 201 "$operator_jar" "$alpha" POST '/api/orders/' "$temporary_root/second-parent.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -d "{\"number\":\"OPS-SECOND\",\"owner_id\":\"$operator_id\"}"
second_parent="$(jq -er '.id' "$temporary_root/second-parent.json")"
jq -cn --arg id "$order_line_id" '{updates: [{id: $id, revision: 1, input: {quantity: 3}}]}' >"$temporary_root/update-lines.json"
assert_status 404 "$operator_jar" "$alpha" POST "/api/orders/$second_parent/_aggregates/lines" "$temporary_root/foreign-child-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H 'If-Match: "rev-1"' --data-binary "@$temporary_root/update-lines.json"

jq '.creates = [.creates[0], .creates[0]]' "$temporary_root/create-lines.json" >"$temporary_root/duplicate-lines.json"
assert_status 422 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/duplicate-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/duplicate-lines.json"
jq --arg parent "$order_id" '.creates[0].input.order_id = $parent' "$temporary_root/create-lines.json" >"$temporary_root/reparent-lines.json"
assert_status 422 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/reparent-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/reparent-lines.json"

assert_status 200 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/extra-line.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/create-lines.json"
extra_line="$(jq -er '.created.first' "$temporary_root/extra-line.json")"
revision="$(jq -er '.parent.revision' "$temporary_root/extra-line.json")"
jq --arg id "$extra_line" --slurpfile update "$temporary_root/update-lines.json" '. + $update[0] + {deletes: [{id: $id, revision: 1}]}' "$temporary_root/create-lines.json" >"$temporary_root/mixed-lines.json"
assert_status 200 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/mixed-response.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/mixed-lines.json"
jq -e --arg id "$order_line_id" '(.rows | length == 2) and ([.rows[] | select(.id == $id and .quantity == 3 and .revision == 2)] | length == 1)' "$temporary_root/mixed-response.json" >/dev/null
extra_line="$(jq -er '.created.first' "$temporary_root/mixed-response.json")"
revision="$(jq -er '.parent.revision' "$temporary_root/mixed-response.json")"
jq -cn --arg id "$extra_line" '{deletes: [{id: $id, revision: 1}]}' >"$temporary_root/remove-extra.json"
assert_status 200 "$operator_jar" "$alpha" POST "$collection_path" "$temporary_root/removed-extra.json" \
  -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/remove-extra.json"
revision="$(jq -er '.parent.revision' "$temporary_root/removed-extra.json")"

jq '.updates[0].revision = 2 | .updates[0].input.quantity = 4' "$temporary_root/update-lines.json" >"$temporary_root/race-lines.json"
race_write() {
  curl --silent --show-error -b "$operator_jar" -o "$temporary_root/race-$1.json" -w '%{http_code}' \
    -H "Content-Type: application/json" -H "X-CSRF-Token: $operator_csrf" -H "X-AppStruct-Tenant: $alpha" \
    -H "If-Match: \"rev-$revision\"" --data-binary "@$temporary_root/race-lines.json" "$api$collection_path" >"$temporary_root/race-$1.status"
}
race_write a &
race_a=$!
race_write b &
race_b=$!
wait "$race_a"
wait "$race_b"
race_status="$(sort "$temporary_root/race-a.status" "$temporary_root/race-b.status" | tr '\n' ' ')"
[[ "$race_status" == '200 412 ' ]]
assert_status 200 "$operator_jar" "$alpha" GET "$collection_path" "$temporary_root/after-race.json"
revision="$(jq -er '.parent.revision' "$temporary_root/after-race.json")"
jq -e '.rows | length == 1 and .[0].quantity == 4 and .[0].revision == 3' "$temporary_root/after-race.json" >/dev/null
echo "Aggregate atomicity, permissions, ownership and concurrent writers passed"
