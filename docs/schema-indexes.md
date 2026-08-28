# Schema Indexes

Entities may declare deterministic composite and partial PostgreSQL indexes:

```yaml
entities:
  User:
    fields:
      organization_id: { type: uuid }
      email: { type: string }
      deleted_at: { type: datetime }
    indexes:
      - fields: [organization_id, email]
      - name: active_user_email
        fields: [email]
        unique: true
        where: deleted_at IS NULL
```

`fields` are listed in PostgreSQL index order and must reference fields declared on the same
entity. `unique: true` creates a unique index; `where` creates a partial index and is treated as a
trusted SQL predicate after compiler validation. Predicates cannot contain semicolons or SQL line
comments. Index names are optional; omitted names are generated from the entity and field list.

Index definitions are included in database snapshots and generated initial migrations. Adding an
index to an existing table is classified as non-destructive but potentially locking, so
`migrate dev` requires explicit review before accepting that migration. Removing or changing an
index is destructive and is never rendered automatically. Migration status also reports missing
or unexpected indexes as schema drift.
