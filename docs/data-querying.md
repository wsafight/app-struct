# Generated Resource Queries

Generated resource collection endpoints support offset pagination for table-style navigation and
cursor pagination for stable traversal of large result sets. Search and declared filters work in
both modes.

## Offset pagination

Offset mode remains the default and includes an exact total:

```text
GET /api/tasks/?page=2&page_size=25&sort=-created_at&filter[status]=todo
```

```json
{
  "data": [],
  "meta": { "page": 2, "page_size": 25, "total": 0 }
}
```

`page` starts at 1. `page_size` defaults to 25 and must be between 1 and 100. Declared sorts are
applied in order and the primary key is appended when needed to keep the ordering deterministic.

## Cursor pagination

Supplying `limit` or `cursor` selects cursor mode. Start a traversal with `limit`; pass the returned
opaque `next_cursor` to continue:

```text
GET /api/tasks/?limit=25&filter[status]=todo
GET /api/tasks/?limit=25&cursor=<next_cursor>&filter[status]=todo
```

```json
{
  "data": [],
  "meta": { "limit": 25, "next_cursor": null, "has_more": false }
}
```

Cursor mode orders by the resource primary key ascending, fetches at most 100 records, and does not
run a total-count query. `page`, `page_size`, and `sort` cannot be combined with cursor mode. Cursor
tokens are versioned Base64URL values and are an API implementation detail; clients must retain
them unchanged. Restart from the first page after changing search or filter parameters.

The generated TypeScript client exposes the modes separately:

```ts
const page = await taskApi.list({ page: 1, page_size: 25 });
const first = await taskApi.listCursor({ limit: 25, filters: { status: "todo" } });
const next = await taskApi.listCursor({
  limit: 25,
  cursor: first.meta.next_cursor ?? undefined,
  filters: { status: "todo" },
});
```

## Relation filters

An outgoing relation can expose one-hop target filters when both the relation field and target
field declare `filterable: true`. For example, a filterable `Task.project` relation and filterable
`Project.status` field produce:

```text
GET /api/tasks/?filter[project.status]=active
GET /api/tasks/?filter[project.created_at][gte]=2026-01-01T00:00:00Z
```

Only generated filter paths published in OpenAPI are accepted. Unsupported parameters fail with an
invalid-query response. Relation filters are implemented as target-key subqueries. The target
entity's list access rule and tenant scope are applied inside each subquery before its value filter,
so result rows and offset totals cannot reveal inaccessible related records.

