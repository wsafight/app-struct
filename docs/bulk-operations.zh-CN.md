# 批量操作

每个生成资源都暴露显式的批量和 CSV 端点。当一次请求要改多条记录、并且调用方需要部分失败明细而不是全部成功或全部回滚时，使用这些端点。

```text
PATCH /api/<table>/_bulk
DELETE /api/<table>/_bulk
GET /api/<table>/_export.csv
POST /api/<table>/_import.csv
```

## 逐条结果

批量写入要求每个 id 都有对应的 `expected_revisions` 条目。授权、租户作用域、Policy、字段检查、钩子和审计事件按记录逐条求值。响应始终包含 `succeeded` 和 `failed` 数组，调用方可以只重试被拒绝的记录。格式错误的 id，以及单条记录的钩子、校验、Policy、冲突或数据库失败会用 savepoint 隔离并报告在 `failed` 中；它们不会回滚其他已成功的记录。内部数据库细节只在服务端记录日志，永远不会包含在响应中。

## CSV

CSV 导出使用 API 字段名并包含表头行。CSV 导入接受相同名称，忽略生成列，通过常规创建契约校验每一行，并在提交成功行的同时报告行级失败。Realtime 事件和 `after_commit` 钩子只在外层事务提交后运行。

## 下一步

- [实体工作流](workflows.zh-CN.md)：工作流字段不会进入批量与 CSV 输入
- [保存视图](saved-views.zh-CN.md)：保存列表状态，不是批量选择
