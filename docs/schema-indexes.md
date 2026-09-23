# Schema Indexes

Entities may declare deterministic composite and partial PostgreSQL indexes. Add these when list
filters or uniqueness rules need an ordered index the compiler can put in snapshots and migrations.

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

## Search indexes

`searchable: true` uses a substring `LIKE` query (`%term%`). A regular B-tree index will not speed
up that pattern on a large table. For high-cardinality or frequently searched fields, install
PostgreSQL's `pg_trgm` extension and create a GIN index in a reviewed manual migration:

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX users_name_search_idx
  ON users USING GIN (name gin_trgm_ops);
```

Declare a matching `indexes` entry in the App Spec and keep the field `searchable`. Replace the
generated B-tree statement with this reviewed operator-owned migration. AppStruct's schema catalog
compares indexed columns, uniqueness, and predicate, while intentionally ignoring the access method
and operator class, so the GIN index remains covered by drift checks.

## Migrations

Index definitions are included in database snapshots and generated initial migrations. Adding an
index to an existing table is classified as non-destructive but potentially locking, so
`migrate dev` requires explicit review before accepting that migration. Removing or changing an
index is destructive and is never rendered automatically. Migration status also reports missing
or unexpected indexes as schema drift.

## See also

- [Migration lint](migration-lint.md) for locking and destructive codes
- [Seed data](seeding.md) for reviewable row inserts in the same snapshot
