# 当前格式契约

只读写当前格式，旧格式与未来格式均明确拒绝。IR 应从 App Spec 重新编译；项目采用 composition-root 布局。迁移 SQL、历史校验和及事务恢复机制仍保留。

<!-- contract-matrix:start -->
| 契约 | 当前版本 |
| --- | ---: |
| `ir` | 18 |
| `runtime_api` | 5 |
| `module_api` | 1 |
| `project_layout` | 2 |
| `database_schema` | 3 |
| `ownership_manifest` | 1 |
| `cache_schema` | 2 |
| `transaction_journal` | 1 |
<!-- contract-matrix:end -->
