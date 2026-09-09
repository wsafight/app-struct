# Relation Display

Declare how related records should appear as labels in generated lists, details, and lookup
controls. Labels never grant extra access.

## Display field

An entity can declare `display_field: number` to use a text, enum, UUID or integer field as its
record label. The compiler resolves the sibling field into Typed IR; unknown or unsupported fields
are rejected. Without a declaration the Web uses the first text field, then the primary key. A
redacted or null display field falls back to the primary key.

## Lookup API

Generated lists and details load related records in batches through
`GET /api/<resource>/_lookup?ids=<comma-separated-ids>`. Each request accepts 1-100 IDs, applies the
target entity's read scope, tenant, soft-delete and custom read Policy, and redacts protected fields.
Unavailable IDs are omitted; order is unspecified. The endpoint never writes data or establishes
ETags for edits. Editors load the current detail before modifying a record.

The generated client exposes `resourceApi.lookup(ids, options)`. Web caches each batch under the
target resource query key, so ordinary mutation invalidation also refreshes related labels. Labels
link to authorized records. Missing and unavailable relations retain their stored identifier.

## See also

- [Resource queries](data-querying.md) — list payloads still expose relation keys, not expanded rows
- [Soft delete and history](soft-delete.md) — lookups honor trash state
