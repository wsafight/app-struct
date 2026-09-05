# Record Activity

Activity v1 adds a collaborative timeline to selected entities while keeping business conversation
separate from immutable Audit history. It supports comments, optional attachments, system events,
creator withdrawal, moderator removal, cursor pagination, and generated detail-page UI.

Auth and Audit are required. Enabling attachments also requires File.

```yaml
modules:
  auth:
    enabled: true
    user_entity: User
  audit:
    enabled: true
    reader_roles: [admin]
  file:
    enabled: true
    allowed_content_types: [text/plain, image/png]
  realtime:
    enabled: true
  activity:
    enabled: true
    max_comment_bytes: 4000
    attachments: true
    admin_roles: [admin]
    resources: [Order]
```

`resources` contains entity names. The generated URL and TypeScript resource key is the entity's
table name, such as `orders`; arbitrary client-provided resource types are rejected. Comment size is
measured in UTF-8 bytes after server-side trimming and control-character removal. The configurable
limit defaults to 4000 bytes and must be between 1 and 65536.

## API And Authorization

For resource `orders` and record `42`, the generated API is:

- `GET /api/activity/orders/42?cursor=...&limit=20`
- `POST /api/activity/orders/42/comments`
- `POST /api/activity/orders/42/{entry_id}/withdraw`
- `POST /api/activity/orders/42/{entry_id}/moderate`
- `GET /api/activity/orders/42/{entry_id}/attachment` when attachments are enabled

Every operation reapplies the target entity's tenant scope, declarative read access, and extension
Policy. A record that is absent or not readable is not exposed through Activity. Mutation endpoints
require cookie authentication and CSRF protection; list and download also support bearer
authentication.

Comment authors may withdraw their own active comments. Actors with an `admin_roles` role may
moderate an active comment and must provide a reason. Both operations preserve a tombstone: body and
attachment metadata are cleared, withdrawal metadata remains, and governance actions are written to
Audit. Attachments accept only name, content type, and base64 content. The server validates them
through File and generates the object key; clients cannot choose a bucket, path, or object key.

Pagination orders by `(occurred_at, id)` descending and returns an opaque `next_cursor`; limits are
1 through 100. Stable Activity-specific errors are `UNKNOWN_ACTIVITY_RESOURCE` (404),
`INVALID_ACTIVITY_INPUT` (422), and `ACTIVITY_ALREADY_WITHDRAWN` (409), alongside normal auth,
tenant, query, and not-found errors.

## System And Realtime Events

Create, update, delete, bulk/CSV writes, soft-delete restore, and Workflow transitions add system
entries in the same database transaction as the business write. Event names are `created`,
`updated`, `deleted`, `restored`, and `workflow.<action>`. System entries store the event name and do
not duplicate Audit before/after snapshots.

When Realtime is enabled, the timeline subscribes to the current record. It refreshes for remote
comment governance, CRUD/restore, and declared Workflow events, deduplicating repeated SSE event IDs.
Without Realtime, the same UI remains available and refreshes after local mutations.

Activity v1 does not implement mentions, reactions, rich text, comment editing, threads, arbitrary
event kinds, or a durable notification inbox. Entries intentionally have no polymorphic database
foreign key to business tables; authorization is enforced by resolving the declared resource and
rechecking its read Policy on every request.
