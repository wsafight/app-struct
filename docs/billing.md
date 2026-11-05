# Generated application billing

Generated applications can opt into Stripe Billing v1 with a tenant scoped organization subscription:

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

The compiler generates the billing tables, organization owner checkout and customer portal routes, a subscription page, OpenAPI and TypeScript clients, and a signed Stripe webhook endpoint. Webhook event IDs are stored before a transaction commits and subscription updates are monotonic by Stripe event time, so retries and out of order delivery do not reapply old access.

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

Stripe Checkout and the Customer Portal handle payment details. The generated application stores provider IDs, subscription status, period end, cancellation state, and plan entitlements. Metered usage is not supported in Billing v1. `appstruct capabilities --format json` reports the supported provider capabilities.
