# Business extension journey

This local demonstration uses public resources to make transaction behavior easy to inspect. Apply your own authentication and access policy before deploying it.

Run `appstruct generate`, configure `DATABASE_URL`, then run `appstruct migrate dev --accept` and `appstruct dev`. Creating a project with `{"code":"OK","name":"Normal project"}` also inserts a review task in the same transaction. Code `DENY` is refused by Policy. Code `FAIL` injects an after-create error: neither project nor task persists. Code `COMMIT` injects an after-commit error: both rows persist and the HTTP create succeeds, with an error in the request's log span.

Use `APPSTRUCT_LOG_FORMAT=json`, retain the response's `X-Request-Id`, and correlate the hook log. This example deliberately reserves those three codes for demonstration. It does not send messages or implement external side effects.

The PostgreSQL regression generates this Spec and executes this exact extension source:

```bash
APPSTRUCT_E2E_DATABASE_URL=postgresql://localhost/appstruct_extension_test \
  cargo test --locked -p appstruct-codegen --test extension_journey
```

See [the extension guide](../../docs/business-extensions.md) for choosing fields, Workflow, Hook/Policy, Command/Query and a custom Web page.
