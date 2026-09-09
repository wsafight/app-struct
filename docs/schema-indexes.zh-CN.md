# Schema 索引

实体可以声明确定性的复合索引和 PostgreSQL 部分索引。当列表筛选或唯一性规则需要编译器写入快照和迁移的有序索引时，使用本页。

```yaml
entities:
  User:
    fields:
      organization_id: { type: uuid }
      email: { type: string }
      deleted_at: { type: datetime }
    indexes:
      - fields: [organization_id, email]
      - name: active_user_email
        fields: [email]
        unique: true
        where: deleted_at IS NULL
```

`fields` 按 PostgreSQL 索引顺序列出，且必须引用同一实体上已声明的字段。`unique: true` 创建唯一索引；`where` 创建部分索引，并在编译器校验后作为受信任的 SQL 谓词处理。谓词不能包含分号或 SQL 行注释。索引名可选；省略时由实体和字段列表生成。

## 迁移

索引定义会进入数据库快照和生成的初始迁移。向现有表添加索引被归类为非破坏性但可能锁表，因此 `migrate dev` 在接受该迁移前需要显式审查。删除或更改索引是破坏性操作，永远不会自动生成。迁移状态也会把缺失或意外的索引报告为 Schema 漂移。

## 下一步

- [迁移检查](migration-lint.zh-CN.md)：锁表与破坏性变更代码
- [Seed 数据](seeding.zh-CN.md)：同一快照中的可审查行插入
