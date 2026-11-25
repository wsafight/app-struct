# B2B CRM & Support Ticketing Starter (Pro)

Multi-tenant B2B CRM + Customer Support Ticketing starter built on the `appstruct/saas` preset.

## Included Capabilities
- **Multi-Tenant + PostgreSQL RLS**: `modules.tenant.rls: true` generates `ENABLE/FORCE ROW LEVEL SECURITY` policies for every tenant-scoped table.
- **Role-Based PII Masking**: Contact `email` and `phone` are dynamically masked in API and CSV responses for roles other than `admin` and `sales_manager`.
- **Read-Only Computed Fields**: `Contact.full_name` is computed deterministically from `{first_name} {last_name}`.
- **Workflow State Machines**: Deal pipeline (`lead -> qualified -> proposal -> won/lost`) and Support Ticket lifecycle (`open -> in_progress -> resolved -> closed`).
- **Declarative File Attachments**: `Ticket.attachments` uses `type: files` with automatic retain/release reference tracking.
- **External HTTP Escalation Hook**: `escalate_ticket` dispatches HMAC-SHA256 signed requests with retries to your external incident/PagerDuty service.
- **Cross-Resource Operational Dashboard**: `GET /api/dashboard` aggregates `Deal` and `Ticket` charts.

## Quickstart
```bash
appstruct check
appstruct build
appstruct dev
```
