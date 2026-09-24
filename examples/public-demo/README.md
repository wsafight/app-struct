# Public Demo Project

This is a reproducible SaaS application for the public product demo. It uses the official `saas`
template and adds a required Project relation to Task. Members can read their own organization's
audit history; the API still denies member access to the admin-only User resource. The tenant
boundary applies to Projects, Tasks, and audit events.

Create a project with the current CLI:

```bash
bash examples/public-demo/create.sh /tmp
cd /tmp/public-demo
appstruct doctor
appstruct dev
```

For a locally built CLI, set `APPSTRUCT_BIN` to its absolute path before running the script.
The script refuses to replace an existing `/tmp/public-demo` project.

In the generated Web app, register a member, create an organization, add a Project, then add a
Task and select that Project. Editing either record creates an event in the Audit log for the
current organization. A member request to `/api/users` must return HTTP 403. Registering another
account and organization must not reveal the first organization's records or audit events.

For public hosting, use the generated `compose.production.yaml` with a production PostgreSQL
database and HTTPS edge. Set `DATABASE_URL`, `APPSTRUCT_FRONTEND_URL`,
`APPSTRUCT_ALLOWED_ORIGIN`, production Auth mail settings, and any required File settings in the
runtime environment. Apply reviewed migrations before starting the API, then run
`node deploy/smoke.mjs <https-web-origin>` and the member journey above. Keep deployment secrets
outside the repository. See [Deployment](../../docs/deployment.md) for the release order.

The docs site cannot host this API because it is static GitHub Pages. Add a live Demo link only
after the application is running at a stable public HTTPS origin.
