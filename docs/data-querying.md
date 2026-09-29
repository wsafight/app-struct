# Generated Resource Queries

Generated resource collection endpoints support offset pagination for table-style navigation and
cursor pagination for stable traversal of large result sets. Search and declared filters work in
both modes. Use this page for the request, response, OpenAPI, and TypeScript client contracts.

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

`page` starts at 1 and must be between 1 and 10,000. `page_size` defaults to 25 and must be between
1 and 100. Search terms are matched as literal substrings; `%`, `_`, and `\` are escaped before the
SQL `LIKE` pattern is applied. Declared sorts are applied in order and the primary key is appended
when needed to keep the ordering deterministic.

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

List and detail payloads currently expose relation fields as foreign-key values. They do not issue
per-record target lookups and therefore do not create an implicit N+1 query path. A future relation
expansion contract must batch target keys, preserve target read policy and tenant scope, and publish
the expanded response shape in OpenAPI and generated TypeScript clients before the Admin table can
display related labels instead of identifiers.

## Aggregates and grouping

Each generated resource exposes a bounded reporting endpoint:

```text
GET /api/tasks/_aggregate?metrics=count,sum:priority,avg:priority&group_by=project.status&limit=20
```

`metrics` is a comma-separated list. `count` (or `count:*`) is always available. Fields marked
`filterable: true` can use `sum` and `avg` when they are integer, bigint, or decimal values; `min`
and `max` additionally support string, enum, date, and datetime fields. `group_by` accepts
filterable scalar fields other than JSON and one-hop to-one relation dimensions such as
`project.status`. At most one relation dimension is allowed per request. Duplicate or unsupported
metrics and groups fail as invalid queries.

```json
{
  "data": [
    {
      "group_project_status": "active",
      "count": 12,
      "sum_priority": 31,
      "avg_priority": 2.5833333333333335
    }
  ],
  "meta": {
    "metrics": ["count", "sum:priority", "avg:priority"],
    "group_by": ["project.status"],
    "limit": 20,
    "order": "desc"
  }
}
```

Result properties use `group_<field>` and `<metric>_<field>` aliases; dots in relation dimensions
become underscores. An omitted `metrics` parameter defaults to `count`. `limit` defaults to 100 and
must be between 1 and 500; it bounds the number of returned groups, not source rows. Groups are
ordered by the first metric (`desc` by default, or `order=asc`) and then by their dimensions, so
Top-N results are deterministic. Search, scalar filters, and one-hop relation filters use the same
parameters as list queries. The source and grouped target entities both apply list access, tenant,
soft-delete, and field-read rules, so aggregates cannot reveal inaccessible data.

The generated TypeScript client accepts arrays and serializes them to the comma-separated wire
format:

```ts
const report = await taskApi.aggregate({
  metrics: ["count", "sum:priority"],
  group_by: ["project.status"],
  limit: 20,
  filters: { "project.status": "active" },
});
```

### Declarative resource charts

Entities can publish validated charts for the generated resource summary:

```yaml
entities:
  Task:
    charts:
      total_tasks:
        label: Total tasks
        type: kpi
        measure: count
      by_project_status:
        label: Tasks by project status
        type: horizontal_bar
        dimension: project.status
        measure: count
        limit: 20
```

Supported types are `kpi`, `bar`, `horizontal_bar`, and `donut`. A KPI has no dimension; the other
types require a filterable scalar field or one-hop to-one relation dimension. Measures use the same
`count`, `sum:<field>`, `avg:<field>`, `min:<field>`, and `max:<field>` contract as the aggregate
endpoint. The compiler rejects unknown paths, deeper relation traversal, incompatible measures,
limits outside 1-100, and more than 20 charts per entity. Date bucketing and line charts remain out
of scope until the aggregate endpoint has an explicit time-bucket contract.

## Field-level access

Fields may declare `access.read` and `access.write` rules. A missing field rule inherits the
entity's operation policy. Read rules are enforced after the row-level scope and before JSON
serialization, so a field that is not visible is omitted from list, detail, and write responses.
Write rules are checked after hooks run and before validation/persistence; submitting a field without
permission returns the same authorization error as the corresponding entity operation. The generated
Web manifest uses these rules to hide unauthorized columns and form controls, while the backend
remains authoritative.

```yaml
api_key:
  type: string
  required: false
  access:
    read: { role: admin }
    write: { role: admin }
```

## See also

- [Scalar values](scalar-values.md) for bigint, decimal, and datetime JSON
- [Saved views](saved-views.md) to persist list query state
- [Relation display](relation-display.md) for batched lookup labels
