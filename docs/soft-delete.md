# Soft Delete and History

Use `soft_delete: true` when delete should archive a row instead of removing it. The entity needs a
nullable `datetime` field named `deleted_at`. Audit events remain the history source.

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
continue to enforce actor and tenant access.

## See also

- [Record activity](activity.md) for authorized history beside the record
- [Saved views](saved-views.md) — list state can include trash mode
- [Relation display](relation-display.md) — lookups honor trash state
