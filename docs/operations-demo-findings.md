# Operations Demo Findings

> Status: P0 accepted; P1 decisions recorded
> Date: 2026-09-05

The canonical Operations Demo and its PostgreSQL/browser harness now exercise Tenant, RBAC, Audit,
Jobs, File, Realtime, Workflow, Report, and Activity in one order lifecycle. The harness covers two
tenants, four roles, Workflow field protection, revision conflict, Report cancellation/retry, File
authorization, Activity, and cross-module tenant/actor binding.

The combined scenario exposed and fixed a backend Policy composition defect: an `any` access rule
was previously ORed with tenant and soft-delete filters, allowing an owner rule to escape tenant
scope. It also established that deterministic Job gating and renderer failure injection can remain
compile-time test support without a production HTTP or CLI control surface.

The resulting P1 decisions are:

| Candidate | Decision | Evidence |
| --- | --- | --- |
| money | accept v1 | amount/currency pairs repeat in SupplierOffer and OrderLine |
| quantity | defer | all useful units require Product relation expansion |
| line items | RFC only | separate OrderLine navigation is awkward, but atomic editing is an aggregate contract |
| role navigation | defer | current Policy-derived visibility works; six resources do not justify grouping syntax |
| production renderer | RFC only | browser rendering needs an isolated threat and resource boundary first |

The accepted money contract is in `docs/business-ui-semantics.md`. The deferred aggregate and
renderer boundaries are in `docs/aggregate-line-items-rfc.md` and
`docs/report-renderer-adapter-rfc.md`.
