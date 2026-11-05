# saas-demo

This project locks `appstruct/saas@1` and adds a team-report SaaS product journey with Billing and Report modules. See `docs/saas-product-journey.zh-CN.md` in the repository for contracts and limitations.

For `database.dev.mode: managed`, ensure Docker with Compose is running. For `external`, set
`DATABASE_URL` in `.env` (see `.env.example`) and run `appstruct migrate dev --accept`.
Generate a fresh report snapshot key with `openssl rand -base64 32` and set
`APPSTRUCT_REPORT_SNAPSHOT_KEY` in your local environment before starting the backend. Keep it stable across restarts and never commit it. Run `appstruct generate` and `appstruct preset show`, then `appstruct doctor` and `appstruct dev`. The generated application starts the built-in Jobs worker for enabled mail/report tasks alongside the API.

The API listens on `http://127.0.0.1:3000` and the Web application on
`http://127.0.0.1:5173` by default. Visit the public landing and pricing pages, register, create an organization, and enter `/studio`. Upload a nonempty text or JSON file (at most 10,000 characters for this report), enter a title, submit and download the result. Each organization has three lifetime free successful reports; queued/running work reserves a slot, canceled/failed work refunds it. `/account` works before creating an organization. Existing tenant-isolated projects/tasks remain available; Audit and business overview readers require `admin`.

New accounts receive the `member` role. After the first operator registers, run
`appstruct auth bootstrap-admin --email admin@example.com` from a trusted host.

Development uses capture mail, PostgreSQL Jobs/Outbox, and local file storage under
`.appstruct/files`. Set production Mail/File provider credentials through environment variables;
never put secrets in `appstruct.yaml`. The preset includes an Admin operations overview; Billing
is added explicitly by this example rather than changing preset version 1. Stripe secrets and prices may all be absent in development: free usage works and checkout is disabled. To test paid access, configure `APPSTRUCT_STRIPE_SECRET_KEY`, `APPSTRUCT_STRIPE_WEBHOOK_SECRET`, and `APPSTRUCT_STRIPE_PRICE_PRO`, then forward signed Stripe events to `/api/billing/stripe/webhook`. Never use a checkout return URL as proof of payment.

The default capture PDF renderer exercises the workflow, not production typography. Set `modules.report.renderer: chromium`, regenerate and combine production Compose with `generated/report-renderer/compose.yaml` before accepting real PDFs. Staged uploads use local proxy or presigned S3 staging keys, with size/MIME validation on completion. Record details expose attachments/replacement/detach, not declarative file fields; attached files retain their original library visibility. Multipart and scanning are not included.

P0–P2 product routes also include `/results`, `/notifications`, `/team`, `/credits`, `/admin/insights`, `/planning`, `/approvals` and optional `/assistant`. Configure a separate stable `APPSTRUCT_AUTH_MFA_KEY` to use account TOTP. AI stays disabled unless the Ollama adapter is explicitly configured; baseline features do not need a model. Pro declares `reports`, `credits` and `seats`. See `docs/saas-product-capabilities.md` for API/security, monthly allowance, top-up/refund, seat and analytics semantics. Monetary metrics are captures/refunds separated by currency, not MRR or bank settlement. Real Stripe/model/S3 services require separate credentials and acceptance.

After generation and Web dependency installation, run `pnpm saas:build` from the repository root to build and prerender `/` and `/pricing`. Set `APPSTRUCT_PUBLIC_SITE_URL` for canonical/sitemap. The normal generated Web build remains available; this extra command is demo-specific, not general SSR. Visitor tracking is first-party and off until consent.

For a production image, create `.env.production` and run `appstruct build` from this example.
Then run `APPSTRUCT_PUBLIC_SITE_URL=https://your-domain.example pnpm saas:build` from the repository root,
before `docker compose -f compose.production.yaml up --build -d --wait` from this example. The Web image copies
the existing `generated/web/dist`; running `appstruct build` after prerendering replaces that HTML, so rerun
`pnpm saas:build` before rebuilding the image. Apply reviewed migrations separately before starting the API image.

Production Web uses a same-origin API proxy on port 8080. Set `APPSTRUCT_FRONTEND_URL` and
`APPSTRUCT_ALLOWED_ORIGIN` to the public HTTPS origin. Compose persists local files in a named
volume. Run `node deploy/smoke.mjs <web-origin>`, then verify login and tenant-scoped CRUD over HTTPS.
The demo proxy disables buffering/caching for authenticated AI SSE as well as realtime events.
