# __APPSTRUCT_PROJECT_NAME__

This project was created from the AppStruct `dashboard` template.

1. Ensure Docker with Compose is running.
2. Run `appstruct doctor`.
3. Run `appstruct dev`.

The API listens on `http://127.0.0.1:3000` and the dashboard on `http://127.0.0.1:5173` by default. Copy `.env.example` to `.env` only when overriding the managed development defaults.

For a production image, create `.env.production`, run `appstruct build`, then use
`docker compose -f compose.production.yaml up -d`. Apply reviewed migrations separately before
starting the API image.
