# __APPSTRUCT_PROJECT_NAME__

This project was created from the AppStruct `minimal` template.

After configuring an external PostgreSQL database, run `appstruct build` and use
`docker compose -f compose.production.yaml up -d` for the API and static Web images. Apply reviewed
migrations separately before starting the API image.

1. Create a PostgreSQL database and set `DATABASE_URL` (see `.env.example`).
2. Run `appstruct doctor`.
3. Run `appstruct dev`.

The API listens on `http://127.0.0.1:3000` and the web app on `http://127.0.0.1:5173` by default.
