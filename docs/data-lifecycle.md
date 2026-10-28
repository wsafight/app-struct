# File and audit lifecycle

## Files

After authorizing the business record, use `ctx.retain_file(key, "app::Order", record_id).await` in its transaction. On record deletion or attachment replacement, use `ctx.release_file` with the same owner. Owners are deduplicated `(entity, record_id)` pairs, limited to 1000. The last release starts retention; repeated release preserves its timestamp. The module checks the tenant; the application owns actor authorization and releasing references when records are deleted. References are not foreign keys.

A trusted maintenance job can call `FileState::collect_unreferenced(30, 100).await`. Batches are limited to 100. It excludes explicit references and active Report/Activity file references. Nothing schedules deletion by default. Review candidates, retention requirements and backups in an isolated database before arranging an application Jobs handler or external maintenance process.

Deletion atomically marks metadata as deleting, holds its row lock while removing the object, removes metadata and commits. New references cannot attach during deletion, and concurrent collectors skip the locked row. Storage failures retain the claim for retry while other candidates continue. Custom provider deletes must be idempotent: concurrent maintenance or retries can delete the same key. Explicit deletion also refuses business references.

Database rollback cannot restore object storage. Release references inside the business transaction and collect after commit. Uploads rolled back before metadata commits can leave objects without metadata; this collector cannot discover them. Reconcile provider inventories against metadata with an allowance for in-flight uploads before manual removal. Direct upload, multipart upload and scanning are separate features.

## Audit

Startup validates `APPSTRUCT_AUDIT_REDACT_FIELDS`, a JSON object mapping entity IDs such as `app::Order` (or `*`) to JSON Pointer arrays. For example: `{"*":["/email"],"app::Order":["/payment/token"]}`. Nested paths, array positions and pointer escaping work; path wildcards do not. Matching values in before, after and metadata are replaced with `[REDACTED]` before insertion. Missing paths are ignored, so field renames require policy updates. Existing events are unchanged. Restart after changing policy.

Set `APPSTRUCT_AUDIT_RETENTION_DAYS=90` and explicitly invoke `appstruct_generated_backend::audit::prune_expired(&database, 1000).await` from trusted maintenance. Unconfigured retention returns zero; configuring it alone never schedules deletion. Batches accept 1–10000 and skip locked events. The application must export and verify any archive before removal; no archive service is included. Audit writes remain part of the business transaction. Existing reader-role and tenant authorization remains in force; do not expose global maintenance to ordinary users.
