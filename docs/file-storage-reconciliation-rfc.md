# File Storage Reconciliation RFC

> Status: implemented in the generated file module. The lifecycle test needs a PostgreSQL database
> (`APPSTRUCT_E2E_DATABASE_URL`) and the built-in local and S3 providers have not been exercised
> against it; see Verification.
> Date: 2026-11-04

## Problem

`FileState::collect_unreferenced` removes objects that have a metadata row in `_appstruct_files`
but no business reference. It cannot see the reverse case: an object in storage with no metadata
row. These orphans accumulate when:

- the process stops after `provider.put` and before the metadata insert commits (`put` writes the
  object first, then inserts metadata, and deletes the object only on an insert error it observes);
- a caller passes its own transaction to `put_with_connection` and that transaction rolls back
  after the object was written;
- metadata rows are restored from an older backup than the storage bucket, or removed by hand.

The first two follow from reading `put`. The third is a plausible cause, not an observed incident.

## Contract

`FileProvider` gains one method with a default body, so existing custom providers keep compiling:

```rust
fn list(&self) -> BoxStream<'static, Result<StoredObject, FileError>>;
// default: a stream that yields one Err(FileError::Unsupported)
// StoredObject { key: String, size: u64, last_modified: DateTime<Utc> }
```

`FileError` gains an `Unsupported` variant. The built-in `ObjectStoreProvider` implements `list`
with `ObjectStore::list`, which covers the local and S3 providers. `object_store` 0.14.2 documents
that listing order is not guaranteed, so the caller consumes one stream in batches and never
relies on key order or a resume key.

`FileState::reconcile_storage(ReconcileOptions) -> Result<ReconcileReport, FileError>` is for trusted maintenance
code, next to `collect_unreferenced`. It is not exposed over HTTP and is not scheduled by default.
The new types are re-exported from the generated crate root, and a project with the file module
disabled keeps the same option and report types and returns `FileError::Disabled`.

| Option | Meaning |
| --- | --- |
| `mode` | `Report` (default) changes nothing. `Delete` removes orphans. |
| `min_age` | Objects modified more recently are never touched. Default 24 hours, minimum 1 hour. |
| `batch_size` | Keys per database lookup, 1 to 1000. Default 500. |
| `max_deletes` | Required in `Delete` mode, 1 to 10000. Caps deletion attempts per pass, including failed attempts. |
| `max_listed` | Orphan keys kept in the report, at most 1000. Counts stay exact. |

Out-of-range options, including ages that exceed the supported timestamp range, return
`FileError::Configuration` before anything is listed.

## Behaviour

1. Walk one listing stream and collect keys into batches. A pass is not resumable; an interrupted
   pass starts over.
2. Skip an object whose `last_modified` is newer than `min_age` (`too_young`). `put` writes the
   object before its metadata becomes visible, so a young object may belong to an upload that has
   not committed. `min_age` must exceed the longest transaction that can call
   `put_with_connection`.
3. Skip a key that fails the module's own key validation (`foreign_keys`). A shared bucket may hold
   objects AppStruct never created.
4. Look the remaining keys up in `_appstruct_files`, one query per batch. A key with no row is an
   orphan. A key with a row, including one marked `deleting`, is left alone.
5. In `Delete` mode, look the key up once more immediately before `provider.delete`, then delete.
   A failed delete is counted in `delete_failures` and does not end the pass.

This narrows the race with a late commit but cannot close it: a row can still commit between the
second lookup and the delete. `min_age` is the guard that matters. The second lookup only shortens
the exposure.

## Out of scope

- Reporting metadata rows whose object is missing from storage. Doing it without loading every row
  needs a second pass over the table; it is left for a later change.
- Deleting such rows. They may point at an object a restore will bring back.
- Running reconciliation on a schedule. Applications register it as a Jobs handler if they want it.
- Cross-bucket or multi-provider reconciliation.
- Tenant attribution. A key without a metadata row has no tenant, so the report is global and the
  call is for operators, not tenant users.

## Risks

- Listing a large bucket is slow and, on S3, billed per request. Memory is bounded by the batch
  size and `max_listed`, but a pass cannot be resumed.
- A `min_age` shorter than the longest open transaction can delete a live upload. This is why the
  minimum is enforced.
- Age is compared against the application host's clock and the provider's `last_modified`. Skew
  between them shifts the cutoff by that amount, which is small against a 1 hour minimum.
- Stale staging files left by a crashed local write may be reported as orphans. That is usually
  correct, but it has not been checked against `LocalFileSystem`.

## Verification

Behaviour is covered by `storage_reconciliation_reports_and_deletes_only_old_orphans` in
`crates/appstruct-codegen/tests/data_lifecycle/runtime.rs`. It uses an in-memory provider, so it
checks the reconciliation logic and the database lookups, not `last_modified` or listing behaviour
of `LocalFileSystem` or S3. It covers:

1. A tracked object, young objects and foreign keys are never orphans.
2. Report mode deletes nothing and counts stay exact when `max_listed` truncates the list.
3. Delete mode removes only old orphans; `max_deletes` caps work per pass.
4. A failing delete is counted and does not stop the pass.
5. A row committed between the batch lookup and the delete saves its object.
6. A provider without `list` returns `Unsupported` and changes nothing.
7. Invalid options are rejected.

`disabled_file_reconciliation_keeps_its_public_contract` also compiles and runs generated code
with the file module disabled, checking that callers can construct the same options and receive
`FileError::Disabled`.

The test returns early when `APPSTRUCT_E2E_DATABASE_URL` is unset, so it only counts when run
against a dedicated test database.
