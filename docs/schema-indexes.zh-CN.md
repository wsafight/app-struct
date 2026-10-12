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

## 搜索与 JSON 索引

在 App Spec 中声明 GIN 方法和 operator class，编译器会直接生成相应迁移：

```yaml
indexes:
  - name: title_search
    fields: [title]
    method: gin
    operator_class: gin_trgm_ops
  - name: payload_search
    fields: [payload]
    method: gin
    operator_class: jsonb_ops
```

`method` 默认 `btree`；`gin` 必须指定 operator class，且不支持 `unique: true`。`gin_trgm_ops` 适用于 string、text 和 enum 字段，生成迁移会创建 `pg_trgm` 扩展，用于加速 `searchable: true` 的子串匹配。`jsonb_ops` 和 `jsonb_path_ops` 适用于 JSON 字段；键存在查询需要 `jsonb_ops`，`jsonb_path_ops` 适用于包含查询。配置与字段类型不兼容时编译失败。

数据库导入和漂移检查包含方法与 operator class，因此同字段的 B-tree 和 GIN 索引会被区分。

## 迁移

索引定义会进入数据库快照和生成的初始迁移。向现有表添加索引被归类为非破坏性但可能锁表，因此 `migrate dev` 在接受该迁移前需要显式审查。删除或更改索引是破坏性操作，永远不会自动生成。迁移状态也会把缺失或意外的索引报告为 Schema 漂移。

## 下一步

- [迁移检查](migration-lint.zh-CN.md)：锁表与破坏性变更代码
- [Seed 数据](seeding.zh-CN.md)：同一快照中的可审查行插入
