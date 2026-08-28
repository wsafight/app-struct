# Soft Delete and History

Set `soft_delete: true` on an entity with a nullable `datetime` field named `deleted_at`:

```yaml
soft_delete: true
fields:
  deleted_at:
    type: datetime
```

Normal list, detail, aggregate, and relation queries exclude trashed rows. Delete keeps the row,
sets `deleted_at`, advances its revision, and records the existing audit event. The generated
resource adds `/_trash` for the authorized trash view and `/_restore` for revision-checked,
per-record restoration. The React list exposes the trash toggle and restore actions. Audit events
remain the history source and continue to enforce actor and tenant access.
