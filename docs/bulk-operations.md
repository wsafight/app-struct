# Bulk Operations

Every generated resource exposes explicit bulk and CSV endpoints:

```text
PATCH /api/<table>/_bulk
DELETE /api/<table>/_bulk
GET /api/<table>/_export.csv
POST /api/<table>/_import.csv
```

Bulk writes require an `expected_revisions` entry for each id. Authorization, tenant scopes,
policies, field checks, hooks, and audit events are evaluated per record. The response always
contains `succeeded` and `failed` arrays so callers can retry only rejected records.

CSV export uses API field names and includes a header row. CSV import accepts the same names,
ignores generated columns, validates each row through the normal create contract, and commits
successful rows together while reporting row-level failures.
