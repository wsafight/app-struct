# Bulk Operations

Every generated resource exposes explicit bulk and CSV endpoints. Use them when many records must
change in one request and callers need a partial-failure report instead of an all-or-nothing write.

```text
PATCH /api/<table>/_bulk
DELETE /api/<table>/_bulk
GET /api/<table>/_export.csv
POST /api/<table>/_import.csv
```

## Per-record results

Bulk writes require an `expected_revisions` entry for each id. Authorization, tenant scopes,
policies, field checks, hooks, and audit events are evaluated per record. The response always
contains `succeeded` and `failed` arrays so callers can retry only rejected records.
Malformed ids and per-record hook, validation, policy, conflict, or database failures are isolated
with a savepoint and reported in `failed`; they do not roll back other successful records. Internal
database details are logged server-side and are never included in the response.

## CSV

CSV export uses API field names and includes a header row. CSV import accepts the same names,
ignores generated columns, validates each row through the normal create contract, and commits
successful rows together while reporting row-level failures. Realtime events and `after_commit`
hooks run only after the outer transaction commits.

CSV export streams rows from a repeatable-read, read-only PostgreSQL snapshot in 64 KiB chunks. It accepts the current search, filters, `where`, `sort`, and `fields`; pagination parameters are rejected because export includes the complete filtered result. Results above 10,000 rows fail before streaming starts, and database statements have a 30-second timeout. Browsers supporting the file picker write chunks directly to the selected file; other browsers use a Blob download.

## See also

- [Entity workflows](workflows.md) — workflow fields are omitted from bulk and CSV inputs
- [Saved views](saved-views.md) for list state, not bulk selection

## Web import and bulk workflows

File selection opens mapping and an editable 25-row preview before any writes. The Web import is bounded to 2 MiB and 10,000 rows; required fields and typed values are checked first. Results retain original CSV record numbers and per-row failures. Download or correct remaining rows, then retry only those rows. Backend result ids are zero-based indices in the submitted batch. Headers must be unique; JSON cells are decoded as JSON values, and malformed JSON fails that row.

Bulk selection is limited to the current page. Updates reuse form controls and exact scalar validation. A review displays the patch, record ids and revisions before submission. Results show each failure; retry refreshes accessible records and requires another review of the write. Successful records are excluded from retries.
