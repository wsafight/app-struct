# Email Verification

Auth-enabled applications issue an email verification link when a user registers and expose a
resend endpoint for signed-in users. Verification tokens are random opaque values stored only as
SHA-256 hashes in `_appstruct_auth_email_verifications`; each token expires after 24 hours and is
consumed once.

## API

- `POST /api/auth/email/request` requires the current session and CSRF token. It is idempotent for
  already verified accounts and replaces a previous pending token otherwise.
- `POST /api/auth/email/verify` accepts `{ "token": "..." }` and marks the matching account's
  `email_verified_at` in the same transaction that consumes the token.
- Auth responses include `email_verified` so clients can show the current state.

The generated React app provides `/verify-email?token=...` and a client method for requesting a new
message. Invalid, expired, or already-used tokens return `400 INVALID_EMAIL_VERIFICATION_TOKEN`.
Mail delivery uses the existing Auth Mail Sender and remains best-effort after the account/token
write; deployments should configure capture only for development and SMTP in production.
