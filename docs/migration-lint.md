# Migration Lint

Run a read-only risk check before accepting or applying a migration:

```bash
appstruct migrate lint
appstruct migrate lint --deny-warnings --format json
```

The command compiles the current App Spec, compares it with `.appstruct/schema.snapshot.json`, and
reports stable issue codes. `AS4201` marks destructive schema or data changes, `AS4202` warns that
an existing-table operation may lock rows, `AS4203` marks operations that need manual SQL review,
and `AS4204` catches a non-null column added without a default or backfill. The command never
writes migrations, snapshots, or database state. It exits non-zero for errors; warnings become
errors when `--deny-warnings` is supplied.
