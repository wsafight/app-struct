# P0–P2 product capabilities

The SaaS demo now extends the unchanged starter preset with the following bounded product slices. Shared contracts are generated; planning, human approvals and the optional AI example remain application-owned. Existing applications must regenerate and review/apply a forward migration, not replace a live database with new initial SQL.

| Priority | Surface | Delivered behavior |
| --- | --- | --- |
| P0 | Files and record details | Staged upload, progress/cancel, local proxy or S3 presigned staging, size/MIME validation, replayable completion, attachments, atomic replacement and reference protection |
| P0 | `/results` | Customer progress, cancel, errors, retention, download and retry as a new run, separate from operator Jobs administration |
| P1 | `/notifications`, `/account` | Inbox/read state/preferences, profile, report defaults, verified email change, password/session/token controls, encrypted TOTP and one-time recovery codes |
| P1 | `/team`, `/credits` | Rename/transfer/remove members, capacity checks at invitation and acceptance, monthly/purchased buckets, ledger, hosted top-up and refund deductions |
| P1 | `/admin/insights` | Consented first-party traffic, registration/activation window counts, fully observed D7 activation retention and currency-separated payment captures/refunds |
| P2 | `/planning`, `/approvals` | Shared Task board/calendar and resource counts; unchanged-task snapshot approval with supporting report, another member and immutable decisions |
| P2 | `/assistant` | Optional Ollama draft example, idempotent credit reservation, Jobs, authenticated SSE snapshots, personal history, cancel/failure refunds and interruption recovery |

Account/inbox pages work before organization creation. Workspace data is tenant-scoped; insights require global administrator access without selecting a tenant.

## Uploads and attachments

`POST /api/files/uploads` accepts `{name, content_type, size}` and returns `{id, upload_url, direct, expires_at}`. PUT bytes to that URL and POST `/api/files/uploads/{id}/complete`. GET the session for status or DELETE it to cancel. Sessions last 15 minutes, are actor/tenant-scoped and limited to 20 pending per actor. Completion returns the same published file on replay. S3 signs only staging keys; completion downloads bounded content, validates MIME/size and creates a separate immutable published key. Configure private-bucket PUT CORS and never send application credentials to the signed URL.

Record routes are `/api/files/records/{resource}/{record_id}` (GET), `.../{file_id}` (PUT/DELETE), and `.../{file_id}/replace` (PUT `{replacement_file_id}`). Resource is the generated table name; typed UUID/integer IDs are canonicalized. Up to 100 attachments per record/file scope. Read/write binding applies generated record read/update access and policies. **Binding does not narrow file-library visibility**: organization-library readers can still download those files. Design a separate private-file scope for stricter confidentiality.

References prevent deletion; hard-deleted records can be pruned, soft deletion preserves references. Failed staging cleanup uses trusted storage reconciliation. Expiry prevents publication but is not automatic metadata retention cleanup. Multipart, scanning and declarative `type: file` fields are not included. Generated OpenAPI/TypeScript describe shared APIs; the original proxied POST remains compatible.

## Account, seats and credits

Security changes require current-password reauthentication and an unused TOTP/recovery code when enabled. Configure and securely retain a separate base64 32-byte `APPSTRUCT_AUTH_MFA_KEY`; rotate only with an encrypted-secret migration. Enabling MFA revokes other sessions while issuing a new current session to display recovery codes. Password/email/session changes and MFA disable revoke sessions and API tokens. Recovery codes are displayed once and stored only as hashes. MFA-enabled OAuth sign-in is refused rather than bypassing the second factor; a full OAuth MFA challenge is not implemented. OAuth-only users need a password before password-reauthenticated actions. Account export/deletion and device inventory are not included.

Profile locale/timezone become defaults only when report creation omits them. Notification preferences affect future events, not history. This is not runtime application-wide language switching.

| Variable | Default / meaning |
| --- | --- |
| `APPSTRUCT_MONTHLY_FREE_CREDITS` | 20 per UTC calendar month, expiring |
| `APPSTRUCT_MONTHLY_PAID_CREDITS` | 200 for valid plans declaring `credits` |
| `APPSTRUCT_TEAM_SEAT_LIMIT`, `APPSTRUCT_TEAM_PAID_SEAT_LIMIT` | 0/unlimited; paid selection requires `seats` |
| `APPSTRUCT_STRIPE_PRICE_CREDITS`, `APPSTRUCT_CREDIT_PACK_UNITS` | Server-owned hosted top-up price and units, default 100 |
| `APPSTRUCT_DAILY_VISIT_EVENT_LIMIT` | 100000 accepted visit events per UTC day |

Monthly credits are spent first. Reservations/refunds commit with business state, are serialized/idempotent and refund to the original bucket without resurrecting expired allowance. Upgrades top up the current grant; downgrades do not claw it back. Purchased balances do not expire. Signed paid Checkout events fulfill known purchases; proportional refunds deduct credits once despite duplicate/reordered events. Refund debt blocks further use until covered. This is not metered Stripe billing, seat-priced subscriptions or invoicing/tax infrastructure. The existing lifetime three-report allowance remains independent.

Traffic is opt-in and deduplicated by visitor/day/page, not raw pageviews. Funnel figures are window counts, not attributed cohort conversion. Captured/refunded sums are currency-separated strings in minor units, before fees, not MRR or bank settlement. No third-party tracker is injected.

## Application-owned routes

`AppExtensions::builder().routes(router)` merges routes under shared CORS/security/metrics layers. Each application handler must authenticate with `context`/`mutation_context` and authorize tenant/business access itself. Route collisions fail startup. Trusted helpers such as `notify` and `reserve_credits` do not authenticate their callers.

- `GET/POST /api/product/approvals`: own submitted/reviewer history (25/page), or UUID `Idempotency-Key` plus `{task_id, report_run_id, reviewer_id, note}`. One pending request per task.
- `POST .../{id}/decide`: `{decision: approved|rejected|cancelled, note}`; reviewer approves/rejects, requester cancels. Matching replay is safe. Approval requires an unchanged task snapshot and atomically completes/audits it with revision advancement.
- `GET .../{id}/history` and `.../{id}/report`: immutable decisions and authorized supporting PDF, subject to report retention, not permanent evidence archival.
- `GET /api/product/assistant/capabilities`, `GET/POST /api/product/assistant`, `GET .../{id}/events`, `POST .../{id}/cancel`: capability status, own paginated history, `{prompt}` with UUID request key, authenticated SSE full snapshots and cancellation.

These scenario-specific APIs have the typed application client in `app/web/collaboration-api.ts`; generated OpenAPI does not automatically infer application routes. They are not a general cross-entity workflow engine or a replacement for arbitrary application hooks/policies. Cross-page immediate realtime delivery is not promised.

AI requires `APPSTRUCT_AI_ENABLED=true`, `APPSTRUCT_AI_MODEL`, and a trusted server-configured Ollama endpoint (`APPSTRUCT_AI_ENDPOINT`, default local `/api/generate`). `APPSTRUCT_AI_MAX_TOKENS` defaults to 512, caps at 2048; input is 8000 UTF-8 bytes, output 64 KiB, two pending per team, timeout 120 seconds. One credit per request; final failure/cancel refunds once. Single-attempt generation avoids automated duplicate external requests; minute reconciliation refunds interrupted/older-than-ten-minute work. SSE rechecks session/membership and renders output as text. Actual model service and output quality require separate operator acceptance.

## Frontend experience

1. **Responsive presentation:** shared product cards, form controls and buttons; compact planning metrics; mobile calendars default to an agenda. The optional month grid scrolls inside a labeled keyboard-focusable region instead of overlapping days or widening the page.
2. **Copy and formatting:** shared translations cover product/marketing pages, navigation, approvals, AI and MFA. Status labels, dates and counts follow UI configuration; currency amounts retain precision, including Stripe's ISK/UGX representation. User data and business labels are not automatically translated. Configure application language in `app/web/setup.ts`; report-language preferences do not switch the whole application.
3. **Queries and refresh:** report consumers, notification badges/inbox, credits and team views share tenant-aware query keys and cancellable reads. Reports/AI poll every two seconds only while the current page contains active work, stop at terminal states and refresh on window focus. Pending approvals refresh every fifteen seconds and stop when idle. Notifications/credits retain their own refresh intervals; this is not real-time push.
4. **Action feedback:** reusable skeletons, empty states, query retries and success notices; downloads, notification reads and task retries/cancellations disable and deduplicate only the relevant row. Failed submissions preserve input. AI prompts display and enforce the actual UTF-8 byte limit.
5. **Navigation:** business pages, data records, team/account and advanced tools are grouped without changing access rules. Mobile navigation closes after selection and supports Escape with focus return; desktop keeps its sidebar.
6. **MFA guidance:** QR generation stays on-device, with a manual setup key and actionable clipboard fallback. Recovery codes can be copied/downloaded and require explicit safe-storage confirmation before dismissal. Unsaved codes guard in-app navigation and refresh/tab closing; secrets/codes are not written to persistent browser storage.

## Public HTML, PDF and validation

After generation/dependency installation, `pnpm saas:build` builds actual anonymous landing/pricing HTML, title/description and robots. An HTTPS-origin `APPSTRUCT_PUBLIC_SITE_URL` adds canonical/sitemap. Build-browser external networking is blocked. This is demo-specific prerendering, not SSR or automatic rendering of every custom public page. Analytics stays off until consent.

For production run `appstruct build` first, then `pnpm saas:build` from the repository root, and only then build the Compose Web image. The image copies `generated/web/dist`; later normal builds overwrite prerendered HTML. The demo Nginx proxy explicitly disables buffering and caching for AI SSE, with periodic server keepalives.

Capture remains development-only. Set `modules.report.renderer: chromium`, regenerate, combine production Compose with `generated/report-renderer/compose.yaml`, keep a stable snapshot key and follow [renderer isolation/acceptance](report-renderer.md). Do not deploy unsandboxed Chromium. Real Stripe collection, real S3 bucket CORS/credentials and target-platform PDF acceptance require the operator's deployment credentials.

`cargo test -p appstruct-codegen --test product_journey` compiles shared APIs and application tests. Database tests skip without `APPSTRUCT_E2E_DATABASE_URL`. For real local validation provide an isolated database, capture mail, temporary file root, test snapshot/MFA keys, seat limit 2 and test Stripe secret/webhook/Pro-price values. Enable AI tests with `APPSTRUCT_AI_ENABLED=true`, model `test-model` and endpoint `http://127.0.0.1:<unused-port>/api/generate`; the test starts its own NDJSON stub, not a real model or Stripe call.

Browser mocks use `APPSTRUCT_E2E_SAAS_PRODUCT=1 PLAYWRIGHT_BASE_URL=http://127.0.0.1:5173 pnpm exec playwright test tests/e2e/saas-product.spec.ts`. Full renderer scripts reset the public schema: use a fresh dedicated database whose name contains test/e2e. Mock browsers, signed local events and model stubs are not external-provider acceptance.

### Local verification (2026-11-06)

Database, PDF and workspace-test results below are the earlier P0–P2 baseline from the same day. This frontend pass reruns Web unit tests, desktop/mobile browser scenarios, typecheck, generated/application-owned ESLint, production build/prerender, generated-tree goldens, the Web contract, Starter compatibility, Rustfmt and workspace Clippy. It does not restart databases or production providers.

| Layer | Verified result |
| --- | --- |
| Workspace regression | `cargo test --workspace` passes; external-database coverage is recorded separately by the real PostgreSQL runs |
| Real PostgreSQL product journey | Shared APIs and application approval/AI tests pass, covering concurrent allowances, refunds, attachments, MFA, seats, signed payment events and tenant isolation; the model is a local NDJSON stub |
| Database round trip | The Demo's initial migration matches inspected PostgreSQL columns, constraints and indexes; all four module-index tests pass |
| Browser and frontend | 14 UI scenarios pass on desktop and 390px mobile Chromium, 28 executions total; 76 generated Web unit tests pass. Coverage includes non-overlapping/contained calendars, terminal polling shutdown, independent notification rows/shared cache, MFA recovery guards/clipboard and UTF-8 prompt limits. Scoped Axe checks pass for account settings/planning; screenshots are visually reviewed. Typecheck, generated/application-owned ESLint and production build pass |
| Public static HTML | Landing/pricing prerender passes; headings, descriptions and canonical links also pass with JavaScript disabled |
| Real PDF | Chromium rendering/download, lease renewal, running cancellation and stale-worker publication protection pass |
| Static quality and stability | Rustfmt, Clippy `-D warnings`, Prettier, generated-tree goldens and Starter compatibility pass |

The local Docker daemon was not running, so production Compose image startup was not accepted. Real Stripe, S3, model services and the target deployment platform still need separate acceptance; stubs and local Chromium are not production certification.
