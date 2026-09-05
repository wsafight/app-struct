# Chromium Report Renderer

The Chromium adapter is implemented as a preview. Local PDF and PostgreSQL lifecycle checks pass.
Linux container acceptance must pass before production rollout; it has not run on the current
macOS host, which has no Docker installation. Capture remains the default.

```yaml
modules:
  report:
    renderer: chromium
```

Regenerate the project. The generated `report-renderer` directory contains pinned Node dependencies,
the renderer, a Dockerfile, an upstream Playwright seccomp profile and a Compose override. With a
prepared production backend and environment, the deployment composition is:

```sh
docker compose -f compose.production.yaml -f generated/report-renderer/compose.yaml config
docker compose -f compose.production.yaml -f generated/report-renderer/compose.yaml up --build -d
```

The backend connects through `APPSTRUCT_REPORT_RENDERER_SOCKET`. A private Unix socket with mode
0660 is shared by UID/GID 10001. The renderer has no backend environment file, database credentials
or File provider access. Its container has no network, a read-only root, dropped capabilities,
no-new-privileges, Chromium sandboxing, 512 MiB memory without swap, 128 Linux tasks, one CPU and
128 MiB temporary storage. One browser request runs at a time; busy requests receive a retryable
adapter-unavailable error. Scale with additional backend/renderer pairs.

Templates are shipped in the App Spec and matched against the generated artifact/version before
rendering. MiniJinja HTML escaping and a fuel limit apply. Inputs cannot supply HTML templates.
The request contains resolved HTML and immutable run/template bindings, not credentials or the
snapshot encryption key. CSS and HTML parsers reject external URLs, imports and active content.
The browser additionally enforces a restrictive CSP, disabled page scripts and request interception.
All network URLs are rejected before resolution, including redirects and DNS-rebinding targets.

The image packages Noto Latin and CJK fonts. Templates may embed PNG/JPEG/WebP images and WOFF/WOFF2/
OTF/TTF fonts as data URLs. SVG, remote resources, custom CSS properties and unsupported raw CSS are
outside the first version. The HTML digest also binds embedded asset bytes.

Limits are 1 MiB snapshot, 2 MiB resolved HTML including inline assets, 100 pages, standard A3/A4/
Letter/Legal dimensions and 30 seconds of rendering. PDF output is bounded by the lower of 50 MiB
and the application's File limit. The worker verifies protocol identity, digest, length and page
count with a PDF parser. Existing Capture templates should be upgraded to HTML and given a new
template version when switching renderer.

Job leases renew during execution. Cancelling a queued or rendering report closes the adapter
connection and prevents publication. Lease loss drops renderer work without overwriting the new
worker's state. Publication locks the run and job, checks ownership, and commits File metadata and
the report result in one transaction. Cancellation racing publication either wins before that lock
or returns a conflict after publication succeeds. Unreferenced objects from a crash are reclaimed
on retry or report retention cleanup.

Blocked resources, invalid artifacts/output, exhausted budgets and invalid snapshots fail without
retry. Adapter unavailability, crashes and timeouts can retry within the configured Jobs budget.
Error responses contain stable codes; report HTML and snapshots are never logged by the adapter.
Drain queued runs before removing or replacing their compiled template versions during an upgrade.

Verification:

```sh
pnpm --dir crates/appstruct-codegen/templates/report-renderer test
bash scripts/run-chromium-report-e2e.sh
bash scripts/run-renderer-isolation.sh
```

The database script requires a dedicated `APPSTRUCT_E2E_DATABASE_URL`; it resets that test database.
The isolation script builds and tests the same image and seccomp settings used in deployment,
including network denial, filesystem permissions, memory exhaustion and PID exhaustion.
