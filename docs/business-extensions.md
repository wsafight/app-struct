# Business extensions and debugging

Choose the smallest existing contract that expresses the requirement:

| Requirement | Extension | Application responsibility |
| --- | --- | --- |
| Ranges, enums, relations, role-based field access | Fields and declarative access | Model, indexes and role acceptance |
| One-record state and authorized transitions | Workflow | Inputs, transition design and revision conflicts |
| Input normalization, transactional side effects, record-dependent authorization | Hook / Policy | Tenant scope and the supplied transaction |
| A separate business action or tailored read | Command / Query handler | Record authorization, query scope and explicit transaction ownership |
| Atomic parent/child editing | Aggregate editing | Detail permissions and revisions |
| Custom interaction or cross-resource composition | React page / headless controller | Routing, cache, loading, errors and conflicts; server authorization still applies |

Workflow is a single-entity state machine. External services require application idempotency, persistent jobs and compensation rather than assumptions about database rollback. Prefer a server extension for calculated values, cross-field rules or dynamic permissions before adding general Spec syntax.

The [business extension example](../examples/business-extension-demo/README.md) inserts a Task from `ProjectHooks::after_create` using `ctx.database()`. Policy rejects code `DENY`. Code `FAIL` fails after inserting the task: both rows roll back. Code `COMMIT` fails after commit: creation succeeds, rows remain and an error is logged. Normal code `OK` succeeds. The example uses public access and reserved fault codes for demonstration; adapt authorization before deployment. Its PostgreSQL regression executes the same extension source and checks response/request-log correlation.

Run `appstruct generate`, configure an isolated database, run `appstruct migrate dev --accept`, then `APPSTRUCT_LOG_FORMAT=json appstruct dev` inside the example. Retain response status, error code and `X-Request-Id`; search JSON logs for that ID and hook fields. Distinguish validation, transactional hooks, Policy and after-commit failures. `before_validate_*` precedes the generated transaction and should normalize or validate input. Use `ctx.database()` for transactional writes. Arbitrary SQL is not automatically tenant-filtered. An after-commit error cannot undo committed work.

Verify actor, tenant, record, ETag and persisted rows in an isolated reproduction. Add a regression for the observed failure and run check, generation checks, build and the relevant journey. Enqueue durable work inside the transaction for reliable external delivery. After-commit network calls are not automatically retried. Command/Query handlers receive request context; create an explicit transaction and bind a context when required.

User-owned `app/backend`, `app/web`, Spec and migrations survive generation/update. Generated ownership checks reject edits in `generated/`. Compile and review extension changes when generated traits change.

Signed remote module packages supply static artifacts and metadata, not arbitrary executable Rust registration. Application Rust is compiled into the composition root and registered through `AppExtensions`. Official runtime modules are generated and started by the framework. These have separate dependency, migration and uninstall responsibilities; see [module registry](module-registry.md). There is no dynamic library or WASM runtime plugin contract.

See [Workflow](workflows.md), [headless controllers](headless-controller.md), [observability](observability.md) and [upgrading](upgrading.md).

## Background job handlers

Without a custom `job_handler`, automatic workers claim only the enabled built-in mail and report kinds. Application jobs remain queued for their registered or external handler. Register a custom handler through `AppExtensions::builder().job_handler(...)` to dispatch application kinds alongside built-ins. Dedicated external workers can use `JobWorker::for_kind(...)` or `JobWorker::for_kinds(...)` to restrict claims to their supported kinds.
