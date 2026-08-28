# OAuth/OIDC Login

Auth-enabled projects can enable the generated OIDC authorization-code flow with:

```yaml
modules:
  auth:
    oauth: true
```

The provider is configured only through environment variables:

- `APPSTRUCT_OIDC_AUTHORIZATION_URL`
- `APPSTRUCT_OIDC_TOKEN_URL`
- `APPSTRUCT_OIDC_USERINFO_URL`
- `APPSTRUCT_OIDC_CLIENT_ID`
- `APPSTRUCT_OIDC_CLIENT_SECRET`
- `APPSTRUCT_OIDC_REDIRECT_URI`

`GET /api/auth/oauth/oidc/start` creates a short-lived HttpOnly state cookie and redirects to the
provider. The callback validates that state, exchanges the code, fetches the OpenID userinfo, and
creates or reuses the local user. Provider subject mappings are stored in
`_appstruct_auth_oauth_accounts`; access and refresh tokens are never persisted. New OIDC users are
marked email-verified because the provider has authenticated the email claim.

The generated login page shows an SSO button when OAuth is enabled. Provider failures return
`502 OAUTH_PROVIDER_ERROR`; missing environment configuration returns
`500 OAUTH_CONFIGURATION_ERROR`.
