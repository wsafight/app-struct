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

## Search and JSON indexes

Declare the GIN method and operator class in the App Spec to generate the corresponding migration:

```yaml
indexes:
  - name: title_search
    fields: [title]
    method: gin
    operator_class: gin_trgm_ops
  - name: payload_search
    fields: [payload]
    method: gin
    operator_class: jsonb_ops
```

`method` defaults to `btree`. `gin` requires an operator class and cannot be unique. `gin_trgm_ops`
supports string, text and enum fields; migrations create the `pg_trgm` extension to accelerate
substring queries on `searchable: true` fields. `jsonb_ops` and `jsonb_path_ops` support JSON fields.
Use `jsonb_ops` for key-existence queries; `jsonb_path_ops` supports containment queries. Incompatible
field types fail compilation.

Database import and drift checks include the method and operator class, distinguishing B-tree and
GIN indexes on the same columns.

## Tenant sort indexes

Enable automatic indexes for tenant-scoped lists in `appstruct.yaml`:

```yaml
modules:
  tenant:
    enabled: true
    sortable_indexes: true
```

For each `tenant: true` entity, the compiler adds a non-unique B-tree index on
`(tenant_id, sortable_field, primary_key)` for every non-primary field marked `sortable: true`.
Entities with `soft_delete: true` get `WHERE deleted_at IS NULL`, using the mapped database column
when `deleted_at` declares `column`, matching generated list queries.
An existing B-tree index with the same columns and predicate already covers the sort, including
a unique index, so the compiler skips the duplicate.

The project setting defaults to `false`, preserving existing migrations. Set `tenant_index: true`
or `tenant_index: false` on an entity to override it; opting in requires `tenant: true`.
Generated SQL index names must fit PostgreSQL's 63-byte limit. For longer names, declare the same
columns and predicate under `indexes` with a shorter explicit name.

These indexes improve tenant filtering and ordering. Lists with page numbers still compute an
exact total; see the [million-row reference run](observability.md#million-row-reference-run) for
the remaining count and deep-offset costs.

## Migrations

Index definitions are included in database snapshots and generated initial migrations. Adding an
index to an existing table is classified as non-destructive but potentially locking, so
`migrate dev` requires explicit review before accepting that migration. Removing or changing an
index is destructive and is never rendered automatically. Migration status also reports missing
or unexpected indexes as schema drift.

## See also

- [Migration lint](migration-lint.md) for locking and destructive codes
- [Seed data](seeding.md) for reviewable row inserts in the same snapshot
