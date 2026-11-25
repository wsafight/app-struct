# AI Application Billing & Job Workbench Starter (Pro)

Multi-tenant AI product workbench starter built on the `appstruct/saas` preset (credits/billing, tenants, MFA, audit, files).

## Included Capabilities
- **Prompt Templates, Datasets & Generation Runs**: Full CRUD + RBAC + tenant RLS isolation (`modules.tenant.rls: true`).
- **Declarative File Upload**: `Dataset.source_file` uses `type: file` for S3/R2 or local file storage with automatic lifecycle retain/release.
- **External Python/Node AI Worker Integration**: `dispatch_inference` calls your external LLM worker via HMAC-SHA256 signed HTTP with 30s timeout and retries — no Rust required for AI logic.
- **Computed Summary & Operational Dashboard**: `GenerationRun.summary_label` + `GET /api/dashboard` combining run status and dataset distribution charts.

## Quickstart
```bash
export AI_WORKER_URL=http://127.0.0.1:8080/infer
export AI_WORKER_HMAC_SECRET=dev-secret
appstruct check
appstruct build
appstruct dev
```
