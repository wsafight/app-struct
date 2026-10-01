# 兼容性契约

持久化与生成契约只在下表范围内兼容，范围外一律失败关闭。当前代码只写入 `current` 版本；
从 `minimum` 到 `current` 的版本由对应 crate 按其策略直接读取或迁移。

<!-- contract-matrix:start -->
| 契约 | 最低版本 | 当前版本 |
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

测试会用 `appstruct-contracts::CONTRACT_MATRIX` 校验此表。修改契约版本时，必须在同一变更中
更新迁移或兼容实现及本文档。
