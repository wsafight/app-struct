# Migration Lint

Run this read-only check before accepting or applying a migration. It never writes migrations,
snapshots, or database state.

```bash
appstruct migrate lint
appstruct migrate lint --deny-warnings --format json
```

The command compiles the current App Spec, compares it with `.appstruct/schema.snapshot.json`, and
reports stable issue codes:

| Code | Meaning |
| --- | --- |
| `AS4201` | Destructive schema or data change |
| `AS4202` | Existing-table operation may lock rows |
| `AS4203` | Operation needs manual SQL review |
| `AS4204` | Non-null column added without a default or backfill |

It exits non-zero for errors. Warnings become errors when `--deny-warnings` is supplied.

## See also

- [Upgrading](upgrading.md) when an update plan is destructive
- [Deployment](deployment.md) for production `migrate status` / `migrate apply`
- [Schema indexes](schema-indexes.md) — adding an index can be a locking warning
