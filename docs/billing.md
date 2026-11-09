# Generated application billing

Generated applications can opt into Billing v1 with a tenant scoped organization subscription. Two providers are supported: `stripe` (default when `provider` is omitted) and `polar`:

```yaml
modules:
  billing:
    enabled: true
    provider: stripe
    capabilities:
      subscriptions: true
      trials: true
      customer_portal: true
    plans:
      - id: pro
        price_env: APPSTRUCT_STRIPE_PRICE_PRO
        trial_days: 14
        entitlements: [projects, seats]
```

The compiler generates the billing tables, organization owner checkout and customer portal routes, a subscription page, OpenAPI and TypeScript clients, and a signed webhook endpoint. Webhook event IDs are stored before a transaction commits and subscription updates are monotonic by provider event time, so retries and out of order delivery do not reapply old access.

## Provider: Polar

Set `provider: polar` to bill through [Polar](https://polar.sh). Polar v1 supports `subscriptions` and `customer_portal`; `trials` and `metered_usage` are rejected at compile time. Each `price_env` must hold a Polar **product UUID** (not a Stripe `price_...` id); the subscription plan resolves from `product_id`, so plan metadata cannot be forged:

```yaml
modules:
  billing:
    enabled: true
    provider: polar
    capabilities:
      subscriptions: true
      customer_portal: true
    plans:
      - id: pro
        price_env: APPSTRUCT_POLAR_PRICE_PRO
        entitlements: [projects, seats]
```

Runtime configuration uses:

- `APPSTRUCT_POLAR_ACCESS_TOKEN` — Organization Access Token sent as a bearer token; keep it server-side only.
- `APPSTRUCT_POLAR_WEBHOOK_SECRET` — the `whsec_…` endpoint secret from the Polar dashboard.
- `APPSTRUCT_POLAR_PRODUCT_CREDITS` — optional product UUID for credit pack purchases.
- one Polar product UUID for every declared `price_env`.

The generated webhook route is `POST /api/billing/polar/webhook`. Deliveries are verified with the Standard Webhooks scheme (`webhook-id`, `webhook-timestamp`, `webhook-signature` headers): the delivery timestamp must be within five minutes, the HMAC-SHA256 signature is compared in constant time, and `webhook-id` deduplicates retries. Subscription state is always re-fetched from the Polar API after signature verification, so a forged or stale payload cannot fabricate access. Organizations are linked through `external_customer_id`.

Business operations can require a declared plan entitlement. The entitlement is checked on the server against the active organization subscription, so a hidden or forged client button cannot bypass the gate:

```yaml
commands:
  GenerateReport:
    input: ReportRequest
    output: Report
    access:
      all:
        - authenticated: true
        - entitlement: reports
```

The entitlement name must appear in at least one configured billing plan. Commands and queries support nested `any`/`all` rules. Denied operation access returns 403, or 401 for anonymous requests. Entitlements require an `active` or `trialing` subscription with an unexpired period. Scheduled cancellation retains access until that period ends.

## Bounded report allowance

Add `entitlement: reports` and `free_runs: 3` to an existing Report module configuration and declare `reports` in a billing plan. `free_runs` requires an entitlement; an entitlement without `free_runs` allows zero free runs. This is a lifetime allowance per organization, independent of the new monthly/purchased credit buckets, not Stripe metered billing. See [product credits and receipts](saas-product-capabilities.md) for top-ups, refunds, debt and seat-capacity semantics.

- Creation reserves usage atomically with the report and job. In-progress reports hold a slot; concurrent submissions cannot exceed the limit.
- Idempotent replay returns the original run without another debit. Reuse the same request key after a network error.
- Successful free reports retain usage. Cancellation and terminal failure refund in the same transaction as the report state update; retrying work keeps its reservation.
- Active paid access bypasses the free allowance. Canceled or expired subscriptions restore the free limit; paid runs do not retroactively consume free usage.
- Report retention does not reset successful usage. A refunded failed run cannot bypass charging through an admin retry; submit a new business operation instead.
- Exhaustion returns HTTP 402 / `QUOTA_EXCEEDED`; a required missing entitlement uses `ENTITLEMENT_REQUIRED`. The billing overview exposes `usage`, `remaining`, and `checkout_available` for upgrade UI.

Persistent database failures, process crashes, and external storage failures still require Jobs/Report operational recovery. This is not a claim of cross-system exactly-once processing or complete crash reconciliation.

Configure secrets only at runtime:

- `APPSTRUCT_STRIPE_SECRET_KEY`
- `APPSTRUCT_STRIPE_WEBHOOK_SECRET`
- one `price_...` value for every declared `price_env`

Development may omit all Stripe secrets and prices: free usage remains available and checkout is explicitly disabled. Partial credentials fail startup; production requires complete configuration. The checkout return page polls subscription state, but never grants access based only on `checkout=success`; signed webhook state remains authoritative.

Stripe Checkout and the Polar/Stripe Customer Portal handle payment details. The generated application stores provider IDs, subscription status, period end, cancellation state, and plan entitlements. Metered usage is not supported in Billing v1, and providers cannot be mixed in one project. `appstruct capabilities --format json` reports the supported provider capabilities.
