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

## Aggregates and grouping

Each generated resource exposes a bounded reporting endpoint:

```text
GET /api/tasks/_aggregate?metrics=count,sum:priority,avg:priority&group_by=status
```

`metrics` is a comma-separated list. `count` (or `count:*`) is always available. Fields marked
`filterable: true` can use `sum` and `avg` when they are integer, bigint, or decimal values; `min`
and `max` additionally support string, enum, date, and datetime fields. `group_by` accepts
filterable scalar fields other than JSON. Duplicate or unsupported metrics and groups fail as
invalid queries.

```json
{
  "data": [
    {
      "group_status": "todo",
      "count": 12,
      "sum_priority": 31,
      "avg_priority": 2.5833333333333335
    }
  ],
  "meta": {
    "metrics": ["count", "sum:priority", "avg:priority"],
    "group_by": ["status"],
    "limit": 100
  }
}
```

Result properties use `group_<field>` and `<metric>_<field>` aliases. An omitted `metrics`
parameter defaults to `count`. `limit` defaults to 100 and must be between 1 and 500; it bounds the
number of returned groups, not source rows. Search, scalar filters, and one-hop relation filters use
the same parameters as list queries. The source entity's list access rule and tenant scope are
applied before aggregation, and relation filters retain their target access scope, so counts and
other metrics cannot include inaccessible records.

The generated TypeScript client accepts arrays and serializes them to the comma-separated wire
format:

```ts
const report = await taskApi.aggregate({
  metrics: ["count", "sum:priority"],
  group_by: ["status"],
  filters: { "project.status": "active" },
});
```
