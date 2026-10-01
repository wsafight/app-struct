# Compatibility Contracts

Persisted and generated contracts fail closed outside the supported ranges below. `current` is the
only version newly written by this checkout; versions from `minimum` through `current` can be read
or migrated according to the owning crate's policy.

<!-- contract-matrix:start -->
| Contract | Minimum | Current |
| --- | ---: | ---: |
| `ir` | 7 | 17 |
| `runtime_api` | 4 | 4 |
| `module_api` | 1 | 1 |
| `project_layout` | 1 | 2 |
| `database_schema` | 1 | 3 |
| `ownership_manifest` | 1 | 1 |
| `cache_schema` | 2 | 2 |
| `transaction_journal` | 1 | 1 |
<!-- contract-matrix:end -->

This table is checked against `appstruct-contracts::CONTRACT_MATRIX`. A contract version change must
update its migration or compatibility implementation and this document in the same change.
