# Realtime Events, Presence, And Edit Leases

The Realtime module provides authenticated server-sent events, PostgreSQL-backed presence, and
optional exclusive edit leases. Every request requires a concrete resource scope. A record scope
also runs that resource's generated read Policy; collection scope runs its list Policy.

```yaml
modules:
  realtime:
    enabled: true
    heartbeat_seconds: 15
    presence_ttl_seconds: 45
```

Subscribe with the generated `subscribeRealtime({ resource, recordId? })` helper. Events are
filtered by tenant, resource, optional record, and the subscriber's current read Policy. Generated
CRUD events expose only `{ resource, record_id }` to the browser; the raw model is retained briefly
in `_appstruct_realtime_events` so each replica can perform row-level authorization before delivery.

Each API process broadcasts local writes immediately and polls the PostgreSQL event table for writes
from other replicas. The expected cross-replica delay is about 100 milliseconds. Rows older than five
minutes are cleaned periodically. This is a live-update channel, not a durable event log: clients
that disconnect or receive a `resync` event must reload their resource query.

Presence rows have a database TTL, are renewed by SSE heartbeat, and are removed on a clean
disconnect. Expired rows are excluded from reads even before cleanup. Resource-level presence lists
do not reveal record-scoped sessions; request the authorized `recordId` explicitly to see them.

The generated lock helpers implement opt-in exclusive leases:

- `acquireRealtimeLock(scope, ttlSeconds)`
- `getRealtimeLock(scope)`
- `renewRealtimeLock(scope, token, ttlSeconds)`
- `releaseRealtimeLock(scope, token)`

TTL is 5 to 300 seconds, default 30. Acquisition returns `409` while another unexpired lease exists;
after expiry, another actor can acquire a new token. Renewal and release require the owning actor,
token, record Policy, and CSRF validation. CRUD does not automatically require a lease, so
applications can adopt locks only for workflows that need exclusive editing.
