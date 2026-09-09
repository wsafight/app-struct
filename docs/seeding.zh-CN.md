# Seed 数据

命名 Seed 行写在领域 Spec 里，并成为生成迁移中可审查、幂等的 SQL。适合随 schema 一起走的引导账户和参考数据。

```yaml
entities:
  User:
    fields:
      id: { type: uuid, primary_key: true, generated: uuid_v7 }
      email: { type: string, required: true }
      active: { type: boolean }
    seeds:
      admin:
        id: 00000000-0000-0000-0000-000000000001
        email: admin@example.com
        active: true
```

Seed 名是限定在实体范围内的稳定标识符。每一行都必须提供主键，以及所有没有默认值的非空字段。标量值会在编译期对照 integer、decimal、boolean 和 enum 字段类型检查。Seed 行会编译进 IR 和数据库 Schema 快照，因此更改或删除一行会在 `migrate plan` 中可见。

## 迁移

初始迁移和安全的开发迁移会把 Seed 行渲染为带 `ON CONFLICT DO NOTHING` 的确定性 SQL，使重试幂等。Seed 插入在初始迁移添加外键约束之前执行，因此父行和子行可以声明在不同实体中。删除或更改已有 Seed 会被视为破坏性数据变更，需要人工审查迁移。

## 下一步

- [Schema 索引](schema-indexes.zh-CN.md)：同一快照中的其他数据库对象
- [迁移检查](migration-lint.zh-CN.md)：Seed 变更属于破坏性时
