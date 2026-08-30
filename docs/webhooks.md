# Signed Webhooks

The Webhooks module stores deliveries in a PostgreSQL outbox and sends them from a background
worker. Publishing is transactional when `RequestContext::publish_webhook` is called with a request
transaction, so rolled-back business writes do not leak outbound events.

```yaml
modules:
  webhooks:
    enabled: true
    poll_interval_ms: 500
    connect_timeout_ms: 3000
    read_timeout_ms: 10000
    request_timeout_ms: 15000
    endpoints:
      operations:
        url: https://hooks.example.com/appstruct
        secret_env: APPSTRUCT_WEBHOOK_OPERATIONS_SECRET
        events: [project.created, project.archived]
        max_attempts: 4
        backoff_seconds: 3
```

Timeout values must be between 100 and 25000 milliseconds. The total request timeout must remain
below the fixed 30-second delivery lease. The worker does not consume or retain response bodies;
only the HTTP status is stored, so an unbounded downstream body cannot consume application memory.
Non-success statuses and transport failures use capped exponential retry and eventually become
`dead`.

Each request includes:

- `X-AppStruct-Delivery`: stable delivery UUID
- `X-AppStruct-Event`: event name
- `X-AppStruct-Timestamp`: Unix timestamp
- `X-AppStruct-Signature`: `v1=` plus lowercase HMAC-SHA256

The signed bytes are `<timestamp>.<raw-body>`. Receivers should use the raw request body, compare the
signature in constant time, reject stale timestamps, and deduplicate by delivery UUID.

Admins can filter recent deliveries at `/admin/webhooks`, retry a dead delivery in place, or replay
a succeeded/dead delivery as a new row. Payloads and signing secrets are intentionally not exposed
by the Admin API. Run multiple workers against the same database safely; claims use leased row locks.
