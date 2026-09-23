# __APPSTRUCT_PROJECT_NAME__

This project was created from the AppStruct `minimal` template.

After configuring an external PostgreSQL database, run `appstruct build` and use
`docker compose -f compose.production.yaml up --build -d --wait` for the API and Web images. Apply reviewed
migrations separately before starting the API image.

For `database.dev.mode: external`, create a PostgreSQL database, set `DATABASE_URL` in `.env`
(see `.env.example`), and run `appstruct migrate dev --accept`. For `managed`, start Docker with
Compose; `appstruct dev` starts the PostgreSQL service. Then run `appstruct doctor` and
`appstruct dev`.

The API listens on `http://127.0.0.1:3000` and the web app on `http://127.0.0.1:5173` by default.

Production Web uses the same-origin API proxy at `http://127.0.0.1:8080`. Configure
`.env.production` before starting containers, then run `node deploy/smoke.mjs http://127.0.0.1:8080`.
