# External HTTP Report Renderer

AppStruct keeps headless browser dependencies out of the core framework. Production PDF generation is delegated to an external HTTP renderer service (such as [Gotenberg](https://gotenberg.dev) or a standalone `appstruct-pdf-renderer` container) using the `renderer: http` plugin contract.

`capture` remains the default renderer for local development and tests.

```yaml
modules:
  report:
    renderer: http
    http_url_env: REPORT_RENDERER_URL
    http_secret_env: REPORT_RENDERER_SECRET
    http_timeout_ms: 30000
```

## Runtime Configuration

| Environment Variable | Required | Description |
| --- | --- | --- |
| `REPORT_RENDERER_URL` (or custom `http_url_env`) | Yes | Base URL of the external PDF rendering service (`POST /render`). |
| `REPORT_RENDERER_SECRET` (or custom `http_secret_env`) | Optional | Shared secret used to sign requests with `x-appstruct-signature: sha256=<hex>` (HMAC-SHA256). |

## Request & Response Contract

- **Request**: `POST <http_url>` with `Content-Type: application/json`, containing `{ run_id, template, version, html, paper_size, orientation }` and an optional `x-appstruct-signature` header.
- **Response**: `200 OK` with `application/pdf` bytes, bounded by `50 MiB` (or the configured `modules.file.max_bytes`, whichever is smaller).
- **Security**: Templates are compiled with MiniJinja HTML escaping. Credentials, database URLs, and snapshot encryption keys are never sent to the renderer.
- **Retries**: Transient network errors, timeouts, and `5xx` responses retry within the configured `modules.jobs` budget; `4xx` responses fail closed without retry.
