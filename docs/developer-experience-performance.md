# Developer Experience Performance

Compiler micro-benchmarks do not capture the wait a developer sees while scaffolding, generating,
installing Web dependencies, or compiling a generated backend. Run the end-to-end benchmark on a
stable machine before a release or after changing build, cache, generator, or dependency behavior:

```bash
bash scripts/run-developer-experience-benchmark.sh
```

The benchmark creates a disposable `minimal` project outside the repository, then measures
scaffolding, cold and warm generation, cold and warm production builds, project cache size, and
generated-tree size. It removes the project on exit and writes the retained result to
`output/benchmarks/developer-experience.json`. Building the CLI itself is deliberately excluded.

Default budgets are deliberately broad enough for ordinary developer laptops: 5 seconds for
scaffolding, 30 seconds for cold generation, 3 seconds for warm generation, 15 minutes for a cold
production build, 2 minutes for a warm build, 8 GiB for project cache, and 1 GiB for generated
output. Override them with `APPSTRUCT_DX_*_BUDGET_MS` or `APPSTRUCT_DX_*_BUDGET_MB`; use
`APPSTRUCT_DX_CLI` to benchmark a specific binary and `APPSTRUCT_DX_OUTPUT` to retain a series.

Compare results only on the same host and toolchain. This benchmark checks the build path before
database startup. Use the deployment and browser E2E runners for PostgreSQL readiness and the
interactive `appstruct dev` journey.
