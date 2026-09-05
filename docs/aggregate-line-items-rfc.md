# Aggregate Line Items

Status: implemented; PostgreSQL and browser acceptance checks passed (2026-09-06).

```yaml
entities:
  Order:
    aggregates:
      lines:
        entity: OrderLine
        relation: order
        states: [draft, rejected]
        max_items: 100
```

The first version edits children of an existing parent. The child has exactly one aggregate owner,
a required relation to that owner, the same tenant scope, a server-generated UUID key and revisions.
Nested aggregates, child workflows and soft deletion are rejected. Parent creation with children,
calculated totals, inventory reservation and a cross-row constraint language are future work.
`states` is required for a parent with a workflow and empty for a parent without one.

Opting in makes ordinary child HTTP mutation endpoints return 405, including bulk and CSV import.
Child reads remain available. All child writes through generated HTTP APIs therefore acquire the
parent lock and apply the parent workflow guard. Application-owned SQL and hooks must preserve this
invariant. Existing data and database storage are unchanged; update child write integrations when
enabling ownership.

`GET /api/orders/{id}/_aggregates/lines` returns `{parent, rows, created}` and the parent ETag.
`POST` to the same endpoint requires `If-Match` and accepts:

```json
{
  "deletes": [{"id": "child-uuid", "revision": 3}],
  "updates": [{"id": "other-child-uuid", "revision": 1, "input": {"quantity": 2}}],
  "creates": [{"key": "local-1", "input": {"product_id": "product-uuid", "quantity": 1}}]
}
```

All arrays default to empty. At least one operation is required. The combined operation budget and
final collection size may not exceed `max_items` (1 to 100). An oversized existing collection is
rejected without returning a partial editor. Duplicate row IDs or create keys are invalid. Create
keys are nonempty strings of at most 128 bytes. Server-generated IDs are returned in `created`,
mapping each submitted key to its new UUID. The parent relation is set by the server and cannot be
supplied in inputs. Unknown or generated input fields are rejected.

Each request uses one transaction:

1. Lock the parent through its read scope and custom read policy; compare its revision.
2. Check the workflow guard, parent update access and custom update policy.
3. Apply deletes and updates sorted by child UUID, then creates in request order. Every child uses
   its read/mutation policies, field authorization, DTO validation and before/after hooks. Validate
   candidate relation targets through their tenant/read/soft-delete scope and custom read policy.
4. Enforce the final collection budget, increment the parent revision and write parent/child Audit
   and child Activity records in the same transaction.
5. Materialize the authorized response before commit. Publish realtime and run after-commit hooks
   only after commit succeeds.

Any failure rolls back every database write, including Audit and Activity. Failed hooks must not
perform irreversible external work; use after-commit hooks or the existing outbox for that work.
Child IDs outside the parent collection are not found. Responses omit unreadable rows and apply
field redaction. Parent updates, workflow transitions and aggregate writes serialize on the same
row lock. Direct child HTTP writes cannot bypass it.

The generated detail editor preserves its draft after validation and revision errors. Reload is an
explicit action that replaces the draft with the current authorized collection. Navigation away
from a dirty editor asks for confirmation. Retrying a committed batch with the old parent revision
returns a conflict, preventing duplicate creations.
