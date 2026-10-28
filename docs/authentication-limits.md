# Authentication and email limits

The generated backend stores one-minute fixed-window counters in PostgreSQL's `_appstruct_auth_login_attempts` table. Each key allows 10 attempts per window, starting with its first request. The next request after expiry starts a new window. Updates are atomic and shared by API replicas.

| Endpoint | Key | When counted |
| --- | --- | --- |
| Login | Normalized email | After input validation, before password verification; successful login clears this key |
| Registration | `register:<normalized email>` | After credential validation, before password hashing or account creation; duplicate-account attempts count |
| Password reset request | `reset:<normalized email>` | Before looking up the account; unknown emails also count and receive the generic response |
| Email verification | `verify:<user ID>` | Before issuing a verification link; registration and unverified-account resends share the counter |

Requests exceeding the limit return HTTP 429, code `RATE_LIMITED`, and `Retry-After: 60`. Wait 60 seconds before retrying. Resends for verified accounts send no email. Successful login clears only its own counter, leaving registration and email counters intact.

These rules limit repeated requests for one email or account. They do not prevent email rotation or infer client addresses from untrusted forwarding headers. Public deployments should define source limits at their proxy or gateway, including the trusted proxy chain and client-address extraction policy. Authenticated endpoints such as personal Token management and tenant invitations retain their own authorization and resource rules.

The regression test connects two independent router instances to PostgreSQL and checks registration, verification mail, concurrent reset requests, 429 responses, and window recovery:

```bash
APPSTRUCT_E2E_DATABASE_URL=<dedicated-test-database> cargo test --locked -p appstruct-codegen --test auth_rate_limit
```
