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

Configure secrets only at runtime:

- `APPSTRUCT_STRIPE_SECRET_KEY`
- `APPSTRUCT_STRIPE_WEBHOOK_SECRET`
- one `price_...` value for every declared `price_env`

Stripe Checkout and the Customer Portal handle payment details. The generated application stores provider IDs, subscription status, period end, cancellation state, and plan entitlements. Metered usage is not supported in Billing v1. `appstruct capabilities --format json` reports the supported provider capabilities.
