# __APPSTRUCT_PROJECT_NAME__

This project was created from the AppStruct `dashboard` template.

For `database.dev.mode: managed`, ensure Docker with Compose is running. For `external`, set
`DATABASE_URL` in `.env` (see `.env.example`) and run `appstruct migrate dev --accept`.
Then run `appstruct doctor` and `appstruct dev`.

The API listens on `http://127.0.0.1:3000` and the dashboard on `http://127.0.0.1:5173` by default.
Set `APPSTRUCT_API_PORT` and `APPSTRUCT_WEB_PORT` in `.env` to change these development defaults.

For a production image, create `.env.production`, run `appstruct build`, then use
`docker compose -f compose.production.yaml up --build -d --wait`. Apply reviewed migrations separately before
starting the API image.

Production Web uses a same-origin API proxy on port 8080. Set `APPSTRUCT_FRONTEND_URL` and
`APPSTRUCT_ALLOWED_ORIGIN` to the public HTTPS origin for Auth applications. Verify the deployment
with `node deploy/smoke.mjs <web-origin>` and an authorized CRUD journey.
