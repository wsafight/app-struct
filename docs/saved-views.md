# Saved Views

Generated resource lists can save the current filter, search, pagination, and sort state as a
named private view in browser storage. Selecting a saved view restores that state and reloads the
list. The copy-link action shares the same state through the normal URL, so a recipient can open a
read-only view without sharing local storage or credentials.

Saved views are scoped to the resource and browser profile. They do not grant access: every list
request still applies the server-side actor, tenant, and resource rules.

There is currently no server-side saved-view entity. Named views do not sync across browsers or
devices, and copying a URL shares only the query state, not a team-editable view. Server-backed
private and shared views remain roadmap work.
