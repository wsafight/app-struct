# Saved Views

Generated resource lists can save the current filter, search, pagination, and sort state as a
named private view in browser storage. Selecting a saved view restores that state and reloads the
list. The copy-link action shares the same state through the normal URL, so a recipient can open a
read-only view without sharing local storage or credentials.

Saved views are scoped to the resource and browser profile. They do not grant access: every list
request still applies the server-side actor, tenant, and resource rules.
