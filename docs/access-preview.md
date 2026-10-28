# Explain declared permissions

`appstruct access preview` compiles the current project and prints a read-only decision matrix. It does not connect to the database or execute application Rust Policy.

```bash
appstruct access preview
appstruct access preview --authenticated --role admin
appstruct --format json access preview --entity Project --actor-id 01990000-0000-7000-8000-000000000001 --record sample-project.json
```

`--role` may repeat for one sample actor and must name declared roles. `--record` contains a local API record (JSON object, at most 1 MiB); `--entity` is required with a record. Output includes CRUD and field-read/ordinary-update rows with the rule, decision and reasons. Field rows combine resource and field access; generated/primary-key/Workflow fields cannot be written through ordinary update. Aggregate children require parent collection writes. Owner rules require the sample actor and owner relation; missing context returns `requires_record`. Tenant resources require `--tenant-id`; a record in another tenant is denied. Sample values are not copied into JSON output.

`scope: declarations_only` and `runtime_policy: not_evaluated` are deliberate boundaries. An `allowed` result does not prove current membership, database state or custom Policy allows access. Use application tests for those checks, as demonstrated in the [business extension example](business-extensions.md). This command never changes roles or grants permissions.
