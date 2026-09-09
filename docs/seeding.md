# Seed Data

Named seed rows live in the domain spec and become reviewable, idempotent SQL in generated
migrations. Use them for bootstrap accounts and reference data that should travel with the schema.

```yaml
entities:
  User:
    fields:
      id: { type: uuid, primary_key: true, generated: uuid_v7 }
      email: { type: string, required: true }
      active: { type: boolean }
    seeds:
      admin:
        id: 00000000-0000-0000-0000-000000000001
        email: admin@example.com
        active: true
```

Seed names are stable identifiers scoped to the entity. Every row must provide the primary key and
all non-nullable fields without a default. Scalar values are checked against integer, decimal,
boolean, and enum field types during compilation. Seed rows are compiled into the IR and database
schema snapshot, so changing or removing a row is visible in `migrate plan`.

## Migrations

Initial migrations and safe development migrations render seed rows as deterministic SQL with
`ON CONFLICT DO NOTHING`, making retries idempotent. Seed inserts run before foreign-key
constraints are added in an initial migration, allowing parent and child rows to be declared in
separate entities. Removing or changing an existing seed is treated as a destructive data change
and requires a manually reviewed migration.

## See also

- [Schema indexes](schema-indexes.md) for other snapshot-owned database objects
- [Migration lint](migration-lint.md) when a seed change is destructive
