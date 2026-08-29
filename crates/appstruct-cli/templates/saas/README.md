# __APPSTRUCT_PROJECT_NAME__

This project was created from the AppStruct `saas` template and locks `appstruct/saas@1`.

1. Ensure Docker with Compose is running.
2. Run `appstruct preset show` to inspect the locked modules.
3. Run `appstruct doctor`.
4. Run `appstruct dev`.

The API listens on `http://127.0.0.1:3000` and the Web application on
`http://127.0.0.1:5173` by default. Create an account, create an organization, and then manage
tenant-isolated projects and tasks. Entity changes are recorded by Audit; the default Audit reader
role is `admin`.

New accounts receive the `member` role. After the first operator registers, run
`appstruct auth bootstrap-admin --email admin@example.com` from a trusted host.

Development uses capture mail, PostgreSQL Jobs/Outbox, and local file storage under
`.appstruct/files`. Set production Mail/File provider credentials through environment variables;
never put secrets in `appstruct.yaml`. Billing and Admin are not included in preset version 1.

For a production image, create `.env.production`, run `appstruct build`, and use
`docker compose -f compose.production.yaml up -d`. Apply reviewed migrations separately before
starting the API image.
