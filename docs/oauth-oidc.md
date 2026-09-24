# OAuth/OIDC Login

Auth-enabled projects can enable the generated OIDC authorization-code flow with:

```yaml
modules:
  auth:
    providers: [google, github]
```

`oauth: true` remains a shorthand for the generic `oidc` provider. Provider IDs are stable and can
be enabled together. The generated UI only renders enabled providers, and the compiler rejects
unknown IDs. Provider credentials are configured only through environment variables:

- `APPSTRUCT_OIDC_AUTHORIZATION_URL`
- `APPSTRUCT_OIDC_TOKEN_URL`
- `APPSTRUCT_OIDC_USERINFO_URL`
- `APPSTRUCT_OIDC_CLIENT_ID`
- `APPSTRUCT_OIDC_CLIENT_SECRET`
- `APPSTRUCT_OIDC_REDIRECT_URI`

Google uses the official OIDC endpoints and requires:

- `APPSTRUCT_GOOGLE_CLIENT_ID`
- `APPSTRUCT_GOOGLE_CLIENT_SECRET`
- `APPSTRUCT_GOOGLE_REDIRECT_URI`

GitHub uses its OAuth API and requires:

- `APPSTRUCT_GITHUB_CLIENT_ID`
- `APPSTRUCT_GITHUB_CLIENT_SECRET`
- `APPSTRUCT_GITHUB_REDIRECT_URI`

`GET /api/auth/oauth/<provider>/start` creates a short-lived HttpOnly state cookie and redirects to
the provider. The callback validates that state, exchanges the code, fetches the provider profile,
and creates or reuses the local user. Provider subject mappings are stored in
`_appstruct_auth_oauth_accounts`; access and refresh tokens are never persisted. New OIDC users are
marked email-verified only when the provider returns the standard boolean claim
`"email_verified": true`. Missing, false, or non-boolean `email_verified` claims reject the login.
Configure generic OIDC and Google to include both `email` and `email_verified` in userinfo. GitHub
uses the `/user/emails` endpoint and accepts only a primary, verified email.

The first verified OIDC login can link an existing local user with the same normalized email.
Only enable providers whose account and email-verification policies are trusted for this purpose;
later logins use the stored provider subject mapping rather than email matching.

The generated login page shows an SSO button when OAuth is enabled. Provider failures return
`502 OAUTH_PROVIDER_ERROR`; missing environment configuration returns
`500 OAUTH_CONFIGURATION_ERROR`.

Use `appstruct capabilities --format json` to inspect the versioned Auth/Billing provider matrix.
