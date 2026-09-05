# Saved Views

Generated resource lists can save their current search, filters, sort, visible columns, page size,
and trash mode as a reusable view. Authenticated applications store private and team views on the
server; browser-only views remain available for local or unauthenticated use. The current list state
can also be copied as a URL without creating a saved view.

Private views are visible only to their creator. Team views require a valid
`X-AppStruct-Tenant` context and are readable by members of that organization. Only the creator can
update or delete either visibility, including a team view. The database uniqueness scope is owner,
tenant scope, resource, and name, so saving an existing owned name updates that view while another
member may keep a view with the same display name.

Server updates and deletes require the latest revision through `If-Match: "rev-<revision>"`. Stale
operations fail with `412 CONCURRENT_MODIFICATION`; missing preconditions fail with `428`. Query
state is opaque but limited to 4096 bytes and is passed back through the generated route validator
when selected.

The API surface is:

- `GET /api/saved-views?resource=<resource>` lists owned views and visible team views.
- `POST /api/saved-views` creates a private or team view.
- `PATCH /api/saved-views/{id}` updates an owned view.
- `DELETE /api/saved-views/{id}` deletes an owned view.
