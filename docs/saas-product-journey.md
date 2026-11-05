# SaaS product journey

`examples/saas-demo` extends the unchanged SaaS preset with application-owned marketing, pricing and report studio pages, plus shared account, file, entitlement and business-overview contracts.

`appstruct new --template saas` remains the baseline starter. The checked-in product demo deliberately adds these capabilities without changing preset version 1 or forcing payments into every new application.

## Page scopes

Pages accept `scope: public | account | workspace` (default: `workspace`). Only public pages can use `path: /`. Public pages require neither authentication nor a tenant. Account pages require authentication but not an organization; workspace pages additionally require a tenant when Tenant is enabled.

The default account page is `/account` and can be replaced by an account-scoped page at that path. Tenant apps also expose `/onboarding`. A public root moves the workspace entry to `/workspace`; otherwise it remains `/`. Login/registration links and refresh preserve a safe local `next`, including its query and hash; external destinations and authentication loops are rejected. Resource paths and generated route prefixes cannot be overridden.

The application registry accepts optional `layouts.public`, `layouts.account` and `layouts.workspace` components with `scope`, `pages`, `resources` and `children`. Render `children` to retain routed content. Registry import currently requires a declared custom page or field component; a layout alone does not trigger import. The demo adds build-time public HTML with `pnpm saas:build`; this is not framework-wide SSR.

## User files

Auth + File applications generate `/files` and authenticated APIs:

- `GET /api/files?page=1&page_size=25`: page 1–10000, size 1–100.
- `POST /api/files?name=...`: raw bytes, file MIME in `Content-Type`, CSRF and current tenant headers.
- `GET /api/files/{id}/download`: private, noncached attachment download.
- `DELETE /api/files/{id}`: CSRF required; business references block deletion with 409.

Tenant applications share files within the current organization; other authenticated applications scope files to the actor. The server checks ID, key prefix and tenant; it does not trust client-supplied object keys. Report outputs are not uploaded-library entries. Uploads reuse Local/S3, MIME/size validation and File reference lifecycle. The UI provides progress, cancellation, pagination, download and deletion confirmation.

The upload UI now uses prepare → upload → complete. Local uploads are proxied; S3 uses presigned staging keys. Completion validates size and MIME and copies to an immutable published key. Sessions expire after 15 minutes, support cancellation and replay completion. Record detail pages expose attachment collections, replacement and detach, not a declarative `type: file` field. Binding does not narrow the original file-library visibility. Multipart and scanning remain out of scope. The studio stores authorized text/JSON content in an encrypted report snapshot rather than asking a worker to read arbitrary object keys.

## Reports and billing

Organizations have three lifetime free successful reports. Pending work reserves a slot; cancellation and terminal failure refund it. Pro's `reports` entitlement enables further submissions. See [billing](billing.md) for atomicity, retry and subscription semantics.

The studio connects upload → input selection → submit → progress → download. Capture Renderer verifies the workflow and simple text output, not production typography or full Unicode rendering. Configure the production Chromium prerequisites and snapshot encryption key as described in [reports](reports.md) and [renderer](report-renderer.md).

## Business overview

`/admin/business` and `GET /api/admin/business` require global `admin`; the API needs no tenant selection. The UI refreshes every minute. Each metric returns definition, source, statistical window and an unavailable reason, with an overview generation timestamp:

- Registered accounts and organizations count currently retained records.
- Successful reports and activated accounts cover retained successful report history, not lifetime activity.
- Paying organizations have a latest active, unexpired subscription; trials are excluded.
- Visits count consented first-party events over 30 days, deduplicated by visitor/UTC day/page, not raw pageviews. Settlement revenue remains unavailable. No third-party tracker is injected.
- `/admin/insights` adds visit/signup/activation window counts, fully observed D7 activation retention and currency-separated captured payments/refunds. These are not attributed conversion, MRR or bank settlement.

## Validation boundaries

The real-PostgreSQL `product_journey` test exercises concurrent allowance reservation, replay, refunds, subscription changes, report download, cross-organization file denial, reference protection and administrator metrics. `tests/e2e/saas-product.spec.ts` uses mocked APIs to verify browser navigation and upload/report/upgrade interaction; it does not certify live Stripe payment.

See [P0–P2 product capabilities](saas-product-capabilities.md) for the new account, inbox, credits, collaboration and optional AI contracts and validation commands.

To run the browser suite, generate the example, install its web dependencies, start its web server, then run from the repository root:

```sh
APPSTRUCT_E2E_SAAS_PRODUCT=1 PLAYWRIGHT_BASE_URL=http://127.0.0.1:5173 \
  pnpm exec playwright test tests/e2e/saas-product.spec.ts
```
