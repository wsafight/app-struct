# Reports

Report v1 provides versioned templates, schema-validated input snapshots, persistent Jobs execution,
PDF publication through File, retention cleanup, and a generated Reports page.

It requires Auth, Jobs, and File. The selected queue must exist, and File must allow
`application/pdf`.

```yaml
modules:
  jobs:
    enabled: true
    queues:
      reports: { max_attempts: 3, backoff_seconds: 5 }
  file:
    enabled: true
    allowed_content_types: [application/pdf]
  report:
    enabled: true
    queue: reports
    max_input_bytes: 262144
    retention_days: 30
    reader_roles: [auditor, admin]
    templates:
      order-summary:
        version: 1
        body: "Order summary: {{ input.order_id }}"
        input_schema: '{"type":"object","required":["order_id"],"properties":{"order_id":{"type":"string","format":"uuid"}},"additionalProperties":false}'
        data_schema_version: 1
```

`max_input_bytes` defaults to 256 KiB and is limited to 4 MiB. `retention_days` defaults to 30 and
is limited to 3650. Template names contain lowercase ASCII letters, digits, `_`, or `-`. Templates
are fixed to PDF in v1; their body digest, JSON Schema, data schema version, and renderer version are
registered together so a released version cannot silently change.

Set `APPSTRUCT_REPORT_SNAPSHOT_KEY` to a base64-encoded 32-byte AES-256-GCM key in every API and
worker process. Inputs are validated, size checked, encrypted with per-run nonces, and stored with a
SHA-256 digest. Missing or invalid key configuration fails closed.

## API And Authorization

- `GET /api/reports/templates`
- `POST /api/reports/templates/{name}/runs`
- `GET /api/reports/runs`
- `GET /api/reports/runs/{id}`
- `POST /api/reports/runs/{id}/cancel`
- `GET /api/reports/runs/{id}/download`

Creation requires a nonempty `Idempotency-Key` of at most 200 characters. Its scope includes tenant,
actor, template, and template version. Reusing it with the same request returns the original run;
reusing it with different data or options returns `REPORT_IDEMPOTENCY_CONFLICT`.

All reads are tenant-bound. A run is visible to its creator and to actors with a configured
`reader_roles` role. Only queued runs can be cancelled. Download repeats run authorization, requires
a succeeded run, and reads the tenant-bound File object. When Audit is enabled, create, cancel, and
download are recorded as `report.create`, `report.cancel`, and `report.download`.

Runs move through `queued`, `rendering`, `publishing`, and `succeeded`; retry exhaustion produces
`failed`, and accepted cancellation produces `cancelled`. A reserved daily Job removes expired
terminal runs and their result files.

The generated client provides typed template names and input shapes for supported JSON Schema
objects. The Reports page creates runs, polls active jobs, cancels queued work, and downloads
completed PDFs.

## V1 Renderer Boundary

The included `capture-v1` renderer is deterministic and intentionally small: it renders MiniJinja
text into a minimal PDF. It is suitable for contract tests and simple text reports, but it is not an
HTML/CSS or Chromium print engine; non-ASCII glyphs are replaced. A sandboxed production browser
renderer, remote assets, images, custom fonts, and tenant-uploaded executable templates remain out
of scope.
