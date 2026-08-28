# Organization Invitations

When the Tenant module is enabled, organization owners can invite existing Auth users to join
their current organization. Invitations are scoped by the `X-AppStruct-Tenant` header and are
available through the generated API and React tenant page.

## API

- `GET /api/tenant/invitations` lists invitations for the current organization.
- `POST /api/tenant/invitations` creates a seven-day invitation. The body accepts an email and an
  optional `role` (currently `member`). Re-sending to the same address replaces an older pending
  invitation. The generated runtime sends the link through the Auth mail sender when configured.
- `DELETE /api/tenant/invitations/{id}` revokes a pending invitation.
- `POST /api/tenant/invitations/{token}/accept` accepts a link for the authenticated user whose
  normalized email matches the invitation. Membership insertion is idempotent and the token can
  only be consumed once.

All management operations require an authenticated organization owner, CSRF validation, and a
valid tenant membership. Invitation tokens are random opaque values; only a SHA-256 hash is stored
in `_appstruct_tenant_invitations`. Expired, revoked, already accepted, or email-mismatched links
return an error without revealing organization membership.

The web runtime exposes an Organization page from the tenant switcher. Owners can send and revoke
invitations and see pending or accepted status. An invitation link opens the acceptance page; after
acceptance the new organization is selected locally.
