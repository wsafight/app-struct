# Personal API Tokens

Auth-enabled applications expose personal bearer tokens for scripts and automation. A token is
created with a display name and an optional lifetime from 1 to 3650 days:

- `GET /api/auth/tokens` lists the current user's token metadata.
- `POST /api/auth/tokens` creates a token and returns its plaintext exactly once.
- `DELETE /api/auth/tokens/{id}` revokes one of the current user's tokens.
- API requests can authenticate with `Authorization: Bearer <token>`.

The generated database stores only a SHA-256 token hash. Expired and revoked tokens are rejected,
and successful bearer requests update `last_used_at`. Token management uses the normal session and
CSRF protections; bearer-authenticated requests do not require a session cookie. The web runtime
adds an API tokens page with copy-once creation and revoke controls, while OpenAPI publishes both
cookie and bearer security schemes.
