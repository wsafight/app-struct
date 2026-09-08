# 聚合行项目

Status: implemented; PostgreSQL and browser acceptance checks passed (2026-09-06).

```yaml
entities:
  Order:
    aggregates:
      lines:
        entity: OrderLine
        relation: order
        states: [draft, rejected]
        max_items: 100
```

第一版编辑已有父级的子项。子项恰好有一个聚合所有者、指向该所有者的必填关系、相同的租户范围、服务端生成的 UUID 键以及修订。嵌套聚合、子工作流和软删除会被拒绝。带子女的父级创建、计算合计、库存预留以及跨行约束语言是后续工作。对带工作流的父级，`states` 是必需的；对没有工作流的父级则为空。

选择加入后，普通子项 HTTP 变更端点会返回 405，包括批量和 CSV 导入。子项读取仍然可用。因此所有通过生成 HTTP API 的子项写入都会获取父级锁并应用父级工作流守卫。应用自有的 SQL 和 hooks 必须保持此不变量。现有数据和数据库存储不变；启用所有权时请更新子项写入集成。

`GET /api/orders/{id}/_aggregates/lines` 返回 `{parent, rows, created}` 以及父级 ETag。向同一端点 `POST` 需要 `If-Match`，并接受：

```json
{
  "deletes": [{"id": "child-uuid", "revision": 3}],
  "updates": [{"id": "other-child-uuid", "revision": 1, "input": {"quantity": 2}}],
  "creates": [{"key": "local-1", "input": {"product_id": "product-uuid", "quantity": 1}}]
}
```

所有数组默认为空。至少需要一次操作。合并操作预算和最终集合大小不得超过 `max_items`（1 到 100）。过大的现有集合会被拒绝，且不返回部分编辑器。重复的行 ID 或创建键无效。创建键是最多 128 字节的非空字符串。服务端生成的 ID 在 `created` 中返回，将每个提交的键映射到其新 UUID。父级关系由服务器设置，不能在输入中提供。未知或 generated 的输入字段会被拒绝。

每个请求使用一个事务：

1. 通过其读范围和自定义读策略锁定父级；比较其修订。
2. 检查工作流守卫、父级更新访问和自定义更新策略。
3. 按子项 UUID 排序应用删除和更新，然后按请求顺序应用创建。每个子项使用其读/变更策略、字段授权、DTO 校验以及 before/after hooks。通过其租户/读/软删除范围和自定义读策略校验候选关系目标。
4. 强制最终集合预算，递增父级修订，并在同一事务中写入父级/子项 Audit 以及子项 Activity 记录。
5. 在提交前物化已授权响应。仅在提交成功后发布 realtime 并运行 after-commit hooks。

任何失败都会回滚每一次数据库写入，包括 Audit 和 Activity。失败的 hooks 不得执行不可逆的外部工作；对此类工作使用 after-commit hooks 或现有 outbox。父级集合之外的子项 ID 视为未找到。响应省略不可读行并应用字段脱敏。父级更新、工作流转换和聚合写入在同一行锁上串行化。直接的子项 HTTP 写入无法绕过它。

生成的详情编辑器在校验和修订错误后保留其草稿。重新加载是显式操作，用当前已授权集合替换草稿。从脏编辑器导航离开会请求确认。用旧的父级修订重试已提交批次会返回冲突，从而防止重复创建。
